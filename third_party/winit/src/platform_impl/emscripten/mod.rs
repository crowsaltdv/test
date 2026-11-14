//! Horizon: winit in a browser through Emscripten (`wasm32-unknown-emscripten`), which upstream
//! winit has no backend for. Its web backend is for `wasm32-unknown-unknown` and wasm-bindgen,
//! and runs on a main thread that may never block; Emscripten gives the program a C library
//! with threads, files and blocking waits, so a native game compiles for it as it is.
//!
//! One window: the page's `<canvas id="canvas">`, as large as the page lays it out. The event
//! loop is the browser's animation frame (`emscripten_set_main_loop_arg`), so `run` never
//! returns. Input comes from Emscripten's HTML5 callbacks, queued until the next frame.
#![cfg(target_os = "emscripten")]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, Weak};
use std::time::{Duration, Instant};

use smol_str::SmolStr;

use crate::dpi::{PhysicalPosition, PhysicalSize};
use crate::event::{
    DeviceEvent, ElementState, InnerSizeWriter, KeyEvent, Modifiers, MouseButton,
    MouseScrollDelta, TouchPhase, WindowEvent,
};
use crate::keyboard::{Key, KeyCode, KeyLocation, ModifiersState, NamedKey, PhysicalKey};

pub(crate) use self::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy, OwnedDisplayHandle};
mod event_loop;
mod ffi;
/// The web backend's tables from a DOM event's `key` and `code` to winit's keys: plain string
/// matching, with nothing of wasm-bindgen in it.
#[allow(dead_code)]
#[path = "../web/keyboard.rs"]
mod dom_keys;

pub use self::window::Window;
mod window;

pub(crate) use crate::cursor::{
    NoCustomCursor as PlatformCustomCursor, NoCustomCursor as PlatformCustomCursorSource,
};
pub(crate) use crate::icon::NoIcon as PlatformIcon;

/// The canvas the window is, as Emscripten's HTML5 functions name it (a CSS selector).
pub(crate) const CANVAS: &CStr = c"#canvas";

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PlatformSpecificEventLoopAttributes {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlatformSpecificWindowAttributes;

/// There is one window, the canvas.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WindowId(u64);

impl WindowId {
    pub(crate) const CANVAS: Self = Self(1);

    pub const fn dummy() -> Self {
        Self(0)
    }
}

impl From<WindowId> for u64 {
    fn from(id: WindowId) -> Self {
        id.0
    }
}

impl From<u64> for WindowId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeviceId;

impl DeviceId {
    pub const fn dummy() -> Self {
        DeviceId
    }
}

#[derive(Clone, Debug)]
pub struct OsError(pub(crate) &'static str);

impl fmt::Display for OsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.0)
    }
}

/// The screen the browser is on.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MonitorHandle;

impl MonitorHandle {
    pub fn name(&self) -> Option<String> {
        Some("Browser".to_owned())
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        let (mut width, mut height) = (0, 0);
        // Safety: writes two integers.
        unsafe { ffi::emscripten_get_screen_size(&mut width, &mut height) };
        let scale = scale_factor();
        PhysicalSize::new(
            (f64::from(width.max(0)) * scale).round() as u32,
            (f64::from(height.max(0)) * scale).round() as u32,
        )
    }

    pub fn position(&self) -> PhysicalPosition<i32> {
        (0, 0).into()
    }

    pub fn scale_factor(&self) -> f64 {
        scale_factor()
    }

    pub fn refresh_rate_millihertz(&self) -> Option<u32> {
        // A page isn't told its screen's refresh rate.
        None
    }

    pub fn video_modes(&self) -> impl Iterator<Item = VideoModeHandle> {
        std::iter::once(VideoModeHandle {
            size: self.size().into(),
            bit_depth: 32,
            refresh_rate_millihertz: 60000,
            monitor: self.clone(),
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct VideoModeHandle {
    size: (u32, u32),
    bit_depth: u16,
    refresh_rate_millihertz: u32,
    monitor: MonitorHandle,
}

impl VideoModeHandle {
    pub fn size(&self) -> PhysicalSize<u32> {
        self.size.into()
    }

    pub fn bit_depth(&self) -> u16 {
        self.bit_depth
    }

    pub fn refresh_rate_millihertz(&self) -> u32 {
        self.refresh_rate_millihertz
    }

    pub fn monitor(&self) -> MonitorHandle {
        self.monitor.clone()
    }
}

/// What `KeyEventExtModifierSupplement` reads.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct KeyEventExtra {
    pub key_without_modifiers: Key,
    pub text_with_all_modifiers: Option<SmolStr>,
}

/// The window's state that any thread may read: the canvas has one of each.
pub(crate) struct Shared {
    /// The canvas in physical pixels.
    width: AtomicU32,
    height: AtomicU32,
    /// `devicePixelRatio`, as the bits of an `f64`.
    scale: AtomicU64,
    pub(crate) redraw: AtomicBool,
    pub(crate) focused: AtomicBool,
    pub(crate) fullscreen: AtomicBool,
    pub(crate) pointer_locked: AtomicBool,
    /// The program asked for the pointer to be locked, and hasn't asked for it back. The browser
    /// grants the lock only to a click or key, so this can be set while `pointer_locked` isn't.
    pub(crate) wants_lock: AtomicBool,
    /// The program hid the cursor (`set_cursor_visible(false)`): it is hidden over the canvas
    /// only while the lock is held, so a lock the browser hasn't granted never leaves the
    /// player with an invisible pointer that stops at the edge of the screen.
    pub(crate) cursor_hidden: AtomicBool,
    /// The cursor the program chose, by its CSS name.
    pub(crate) cursor_icon: Mutex<String>,
    pub(crate) window: AtomicBool,
    /// Something for the loop to do while its control flow is `Wait`.
    pub(crate) wake: AtomicBool,
    pub(crate) title: Mutex<String>,
}

pub(crate) static SHARED: Shared = Shared {
    width: AtomicU32::new(0),
    height: AtomicU32::new(0),
    scale: AtomicU64::new(0),
    redraw: AtomicBool::new(false),
    focused: AtomicBool::new(true),
    fullscreen: AtomicBool::new(false),
    pointer_locked: AtomicBool::new(false),
    wants_lock: AtomicBool::new(false),
    cursor_hidden: AtomicBool::new(false),
    cursor_icon: Mutex::new(String::new()),
    window: AtomicBool::new(false),
    wake: AtomicBool::new(false),
    title: Mutex::new(String::new()),
};

pub(crate) fn inner_size() -> PhysicalSize<u32> {
    PhysicalSize::new(SHARED.width.load(Ordering::Relaxed), SHARED.height.load(Ordering::Relaxed))
}

pub(crate) fn scale_factor() -> f64 {
    match f64::from_bits(SHARED.scale.load(Ordering::Relaxed)) {
        scale if scale > 0.0 => scale,
        // Before the first window.
        // Safety: reads `devicePixelRatio`.
        _ => unsafe { ffi::emscripten_get_device_pixel_ratio() }.max(0.1),
    }
}

/// Asks the browser to lock the pointer to the canvas. It does so now only while the page has
/// the user's activation (a click or key a few seconds ago, or this very event); true when the
/// request was made. Without activation it waits for `on_mouse` or `on_key` to ask again.
pub(crate) fn request_lock() -> bool {
    // Safety: the selector is NUL-terminated.
    unsafe { ffi::emscripten_request_pointerlock(CANVAS.as_ptr(), false) == ffi::RESULT_SUCCESS }
}

/// An event waiting for the next frame.
pub(crate) enum Queued {
    Window(WindowEvent),
    Device(DeviceEvent),
}

/// What the callbacks remember between events. They and the loop run on the main thread.
#[derive(Default)]
struct Input {
    modifiers: ModifiersState,
    /// When Escape last came from the keyboard, and when the loss of the pointer lock stood in
    /// for it: one press must not count twice.
    escape_key: Option<Instant>,
    escape_made: Option<Instant>,
    /// The mouse buttons (bit per button) whose press only took the pointer lock: the program
    /// hears neither the press nor its release.
    swallowed: u8,
}

thread_local! {
    static QUEUE: RefCell<VecDeque<Queued>> = const { RefCell::new(VecDeque::new()) };
    static INPUT: RefCell<Input> = RefCell::new(Input::default());
}

fn push(event: WindowEvent) {
    QUEUE.with(|q| q.borrow_mut().push_back(Queued::Window(event)));
    SHARED.wake.store(true, Ordering::Relaxed);
}

fn push_device(event: DeviceEvent) {
    QUEUE.with(|q| q.borrow_mut().push_back(Queued::Device(event)));
    SHARED.wake.store(true, Ordering::Relaxed);
}

pub(crate) fn take_queue() -> VecDeque<Queued> {
    QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Run a line of JavaScript on the page, for the two things Emscripten's HTML5 functions leave
/// out (the canvas's cursor). Main thread only: a worker has no document.
pub(crate) fn run_script(script: &str) {
    if let Ok(script) = CString::new(script) {
        // Safety: a NUL-terminated string, read before the call returns.
        unsafe { ffi::emscripten_run_script(script.as_ptr()) };
    }
}

/// Reads the canvas's size on the page and gives the canvas as many pixels. `Some` when the
/// size or the scale changed: the new scale, when it did.
pub(crate) fn sync_size() -> Option<(PhysicalSize<u32>, Option<f64>)> {
    let (mut css_width, mut css_height) = (0.0, 0.0);
    // Safety: writes two doubles; the selector is NUL-terminated.
    let found = unsafe {
        ffi::emscripten_get_element_css_size(CANVAS.as_ptr(), &mut css_width, &mut css_height)
    };
    if found != ffi::RESULT_SUCCESS {
        return None;
    }
    // Safety: reads `devicePixelRatio`.
    let scale = unsafe { ffi::emscripten_get_device_pixel_ratio() };
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let width = ((css_width * scale).round() as u32).max(1);
    let height = ((css_height * scale).round() as u32).max(1);
    let old_scale = f64::from_bits(SHARED.scale.swap(scale.to_bits(), Ordering::Relaxed));
    let old_width = SHARED.width.swap(width, Ordering::Relaxed);
    let old_height = SHARED.height.swap(height, Ordering::Relaxed);
    if (old_width, old_height) != (width, height) {
        // Safety: the selector is NUL-terminated.
        unsafe {
            ffi::emscripten_set_canvas_element_size(CANVAS.as_ptr(), width as c_int, height as c_int)
        };
    }
    let scaled = (old_scale != scale && old_scale > 0.0).then_some(scale);
    ((old_width, old_height) != (width, height) || scaled.is_some())
        .then_some((PhysicalSize::new(width, height), scaled))
}

/// Queues what a changed canvas size means to the program.
pub(crate) fn resized() {
    if !SHARED.window.load(Ordering::Relaxed) {
        return;
    }
    if let Some((size, scale)) = sync_size() {
        if let Some(scale_factor) = scale {
            push(WindowEvent::ScaleFactorChanged {
                scale_factor,
                inner_size_writer: InnerSizeWriter::new(Weak::new()),
            });
        }
        push(WindowEvent::Resized(size));
        SHARED.redraw.store(true, Ordering::Relaxed);
    }
}

fn text_of(field: &[c_char]) -> &str {
    // Safety: `c_char` and `u8` have one layout; the slice is the field's own bytes.
    let bytes = unsafe { std::slice::from_raw_parts(field.as_ptr().cast::<u8>(), field.len()) };
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).unwrap_or("")
}

fn modifiers_of(ctrl: bool, shift: bool, alt: bool, meta: bool) -> ModifiersState {
    let mut state = ModifiersState::empty();
    state.set(ModifiersState::CONTROL, ctrl);
    state.set(ModifiersState::SHIFT, shift);
    state.set(ModifiersState::ALT, alt);
    state.set(ModifiersState::SUPER, meta);
    state
}

/// Tells the program the modifiers an event carried, when they changed.
fn set_modifiers(state: ModifiersState) {
    let changed = INPUT.with(|i| std::mem::replace(&mut i.borrow_mut().modifiers, state) != state);
    if changed {
        push(WindowEvent::ModifiersChanged(Modifiers::from(state)));
    }
}

/// The key a physical key gives with no modifier held, as far as a page can tell: the page gets
/// the key with its modifiers, and the layout isn't its to read. A letter is its lower case; a
/// digit or a sign key is its US one; anything else is the key as it came.
fn key_without_modifiers(physical: PhysicalKey, logical: &Key) -> Key {
    if let Key::Character(text) = logical {
        let mut chars = text.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            if c.is_alphabetic() {
                return Key::Character(c.to_lowercase().collect::<String>().into());
            }
        }
    }
    let PhysicalKey::Code(code) = physical else {
        return logical.clone();
    };
    let plain = match code {
        KeyCode::Digit0 => "0",
        KeyCode::Digit1 => "1",
        KeyCode::Digit2 => "2",
        KeyCode::Digit3 => "3",
        KeyCode::Digit4 => "4",
        KeyCode::Digit5 => "5",
        KeyCode::Digit6 => "6",
        KeyCode::Digit7 => "7",
        KeyCode::Digit8 => "8",
        KeyCode::Digit9 => "9",
        KeyCode::Minus => "-",
        KeyCode::Equal => "=",
        KeyCode::BracketLeft => "[",
        KeyCode::BracketRight => "]",
        KeyCode::Backslash => "\\",
        KeyCode::Semicolon => ";",
        KeyCode::Quote => "'",
        KeyCode::Backquote => "`",
        KeyCode::Comma => ",",
        KeyCode::Period => ".",
        KeyCode::Slash => "/",
        _ => return logical.clone(),
    };
    match logical {
        Key::Character(_) => Key::Character(plain.into()),
        _ => logical.clone(),
    }
}

fn key_event(physical_key: PhysicalKey, logical_key: Key, pressed: bool, repeat: bool) -> KeyEvent {
    key_event_at(physical_key, logical_key, KeyLocation::Standard, pressed, repeat, false)
}

fn key_event_at(
    physical_key: PhysicalKey,
    logical_key: Key,
    location: KeyLocation,
    pressed: bool,
    repeat: bool,
    command: bool,
) -> KeyEvent {
    let typed = pressed.then(|| logical_key.to_text().map(SmolStr::new)).flatten();
    KeyEvent {
        physical_key,
        text: if command { None } else { typed.clone() },
        location,
        state: if pressed { ElementState::Pressed } else { ElementState::Released },
        repeat,
        platform_specific: KeyEventExtra {
            key_without_modifiers: key_without_modifiers(physical_key, &logical_key),
            text_with_all_modifiers: typed,
        },
        logical_key,
    }
}

/// How close a real Escape and the loss of the pointer lock must be to be one press.
const SAME_ESCAPE: Duration = Duration::from_millis(250);

extern "C" fn on_key(kind: c_int, event: *const ffi::KeyboardEvent, _: *mut c_void) -> bool {
    // Safety: Emscripten passes a valid event for the length of the call.
    let event = unsafe { &*event };
    let pressed = kind == ffi::EVENT_KEYDOWN;
    let (key, code) = (text_of(&event.key), text_of(&event.code));
    let physical_key = PhysicalKey::from_key_code_attribute_value(code);
    let logical_key = Key::from_key_attribute_value(key);
    let location = match event.location {
        1 => KeyLocation::Left,
        2 => KeyLocation::Right,
        3 => KeyLocation::Numpad,
        _ => KeyLocation::Standard,
    };
    // Control alone, or the command key, makes a shortcut and types nothing; Control with Alt
    // is AltGr on Windows, which types.
    let command = (event.ctrl_key && !event.alt_key) || event.meta_key;
    let escape = physical_key == PhysicalKey::Code(KeyCode::Escape);
    let twice = escape
        && pressed
        && INPUT.with(|i| {
            let mut input = i.borrow_mut();
            let now = Instant::now();
            let twice = input.escape_made.is_some_and(|at| now.duration_since(at) < SAME_ESCAPE);
            input.escape_key = Some(now);
            twice
        });
    if !twice {
        push(WindowEvent::KeyboardInput {
            device_id: crate::event::DeviceId(DeviceId),
            event: key_event_at(
                physical_key,
                logical_key,
                location,
                pressed,
                event.repeat,
                command,
            ),
            is_synthetic: false,
        });
    }
    set_modifiers(modifiers_of(event.ctrl_key, event.shift_key, event.alt_key, event.meta_key));
    // The browser keeps its own keys for its developer tools and for pasting (the page is
    // given the text only if the default action runs); every other key is the game's.
    // A key is the activation a lock needs (Escape is not: the browser keeps it for itself).
    if !escape
        && SHARED.wants_lock.load(Ordering::Relaxed)
        && !SHARED.pointer_locked.load(Ordering::Relaxed)
    {
        request_lock();
    }
    let dev_tools = physical_key == PhysicalKey::Code(KeyCode::F12)
        || (event.ctrl_key && event.shift_key && matches!(code, "KeyI" | "KeyJ" | "KeyC"));
    let paste = command && code == "KeyV";
    !(dev_tools || paste)
}

fn button_of(button: u16) -> MouseButton {
    match button {
        0 => MouseButton::Left,
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        3 => MouseButton::Back,
        4 => MouseButton::Forward,
        other => MouseButton::Other(other),
    }
}

extern "C" fn on_mouse(kind: c_int, event: *const ffi::MouseEvent, _: *mut c_void) -> bool {
    // Safety: Emscripten passes a valid event for the length of the call.
    let event = unsafe { &*event };
    let device_id = crate::event::DeviceId(DeviceId);
    set_modifiers(modifiers_of(event.ctrl_key, event.shift_key, event.alt_key, event.meta_key));
    let locked = SHARED.pointer_locked.load(Ordering::Relaxed);
    let moved = || {
        let scale = scale_factor();
        WindowEvent::CursorMoved {
            device_id,
            position: PhysicalPosition::new(
                f64::from(event.target_x) * scale,
                f64::from(event.target_y) * scale,
            ),
        }
    };
    match kind {
        ffi::EVENT_MOUSEMOVE => {
            // A locked pointer has no place on the page, only its movement.
            if !locked {
                push(moved());
            }
            if event.movement_x != 0 || event.movement_y != 0 {
                push_device(DeviceEvent::MouseMotion {
                    delta: (f64::from(event.movement_x), f64::from(event.movement_y)),
                });
            }
        },
        ffi::EVENT_MOUSEDOWN | ffi::EVENT_MOUSEUP => {
            let bit = 1u8 << event.button.min(7);
            // The program wants the pointer and the browser hasn't given it: this press is the
            // click that does, and is not the program's (it would break a block). Its release
            // goes the same way.
            let swallow = INPUT.with(|input| {
                let mut input = input.borrow_mut();
                if kind == ffi::EVENT_MOUSEDOWN {
                    let take = SHARED.wants_lock.load(Ordering::Relaxed) && !locked;
                    if take {
                        input.swallowed |= bit;
                    }
                    take
                } else {
                    let had = input.swallowed & bit != 0;
                    input.swallowed &= !bit;
                    had
                }
            });
            if swallow {
                if kind == ffi::EVENT_MOUSEDOWN {
                    request_lock();
                }
                return true;
            }
            // A release is heard on the whole page, where its place isn't the canvas's.
            if !locked && kind == ffi::EVENT_MOUSEDOWN {
                push(moved());
            }
            push(WindowEvent::MouseInput {
                device_id,
                state: if kind == ffi::EVENT_MOUSEDOWN {
                    ElementState::Pressed
                } else {
                    ElementState::Released
                },
                button: button_of(event.button),
            });
        },
        ffi::EVENT_MOUSEENTER => {
            push(WindowEvent::CursorEntered { device_id });
            push(moved());
        },
        ffi::EVENT_MOUSELEAVE => push(WindowEvent::CursorLeft { device_id }),
        _ => {},
    }
    true
}

extern "C" fn on_context_menu(_: c_int, _: *const ffi::MouseEvent, _: *mut c_void) -> bool {
    // The right button is the game's: no browser menu over the canvas.
    true
}

extern "C" fn on_wheel(_: c_int, event: *const ffi::WheelEvent, _: *mut c_void) -> bool {
    // Safety: Emscripten passes a valid event for the length of the call.
    let event = unsafe { &*event };
    // A page scrolls down for a positive delta; winit's is positive away from the user.
    let delta = match event.delta_mode {
        ffi::DOM_DELTA_PIXEL => {
            let scale = scale_factor();
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(
                -event.delta_x * scale,
                -event.delta_y * scale,
            ))
        },
        _ => MouseScrollDelta::LineDelta(-event.delta_x as f32, -event.delta_y as f32),
    };
    push(WindowEvent::MouseWheel {
        device_id: crate::event::DeviceId(DeviceId),
        delta,
        phase: TouchPhase::Moved,
    });
    true
}

extern "C" fn on_resize(_: c_int, _: *const ffi::UiEvent, _: *mut c_void) -> bool {
    resized();
    false
}

extern "C" fn on_focus(kind: c_int, _: *const ffi::FocusEvent, _: *mut c_void) -> bool {
    let focused = kind == ffi::EVENT_FOCUS;
    if SHARED.focused.swap(focused, Ordering::Relaxed) != focused {
        if !focused {
            // The keys released while another window had the keyboard never reach the page.
            set_modifiers(ModifiersState::empty());
        }
        push(WindowEvent::Focused(focused));
    }
    false
}

extern "C" fn on_fullscreen(
    _: c_int,
    event: *const ffi::FullscreenChangeEvent,
    _: *mut c_void,
) -> bool {
    // Safety: Emscripten passes a valid event for the length of the call.
    let event = unsafe { &*event };
    SHARED.fullscreen.store(event.is_fullscreen, Ordering::Relaxed);
    resized();
    false
}

extern "C" fn on_pointer_lock(
    _: c_int,
    event: *const ffi::PointerlockChangeEvent,
    _: *mut c_void,
) -> bool {
    // Safety: Emscripten passes a valid event for the length of the call.
    let event = unsafe { &*event };
    let held = SHARED.pointer_locked.swap(event.is_active, Ordering::Relaxed);
    // The cursor goes with the lock, not with the program's request for it.
    window::apply_cursor();
    // The browser took the pointer back while the program still wanted it: the player pressed
    // Escape, which a browser spends on releasing the pointer and may not pass on, or left the
    // page. Either way the program hears Escape, as it would from a window.
    if !event.is_active && held && SHARED.wants_lock.swap(false, Ordering::Relaxed) {
        let twice = INPUT.with(|i| {
            let mut input = i.borrow_mut();
            let now = Instant::now();
            let twice = input.escape_key.is_some_and(|at| now.duration_since(at) < SAME_ESCAPE);
            input.escape_made = Some(now);
            twice
        });
        if !twice {
            for pressed in [true, false] {
                push(WindowEvent::KeyboardInput {
                    device_id: crate::event::DeviceId(DeviceId),
                    event: key_event(
                        PhysicalKey::Code(KeyCode::Escape),
                        Key::Named(NamedKey::Escape),
                        pressed,
                        false,
                    ),
                    is_synthetic: false,
                });
            }
        }
    }
    false
}

extern "C" fn on_visibility(
    _: c_int,
    event: *const ffi::VisibilityChangeEvent,
    _: *mut c_void,
) -> bool {
    // Safety: Emscripten passes a valid event for the length of the call.
    let event = unsafe { &*event };
    push(WindowEvent::Occluded(event.hidden));
    false
}

/// Listens to the page for the window, once: the canvas for the pointer, the page's window for
/// the keyboard, so the canvas needn't have the focus.
pub(crate) fn listen() {
    static LISTENING: AtomicBool = AtomicBool::new(false);
    if LISTENING.swap(true, Ordering::Relaxed) {
        return;
    }
    let (data, thread, canvas) = (std::ptr::null_mut(), ffi::CALLING_THREAD, CANVAS.as_ptr());
    // Safety: the callbacks are functions of the program, which live as long as the page, and
    // the targets are a NUL-terminated selector or Emscripten's names for the page's objects.
    unsafe {
        ffi::emscripten_set_keydown_callback_on_thread(
            ffi::TARGET_WINDOW,
            data,
            true,
            Some(on_key),
            thread,
        );
        ffi::emscripten_set_keyup_callback_on_thread(
            ffi::TARGET_WINDOW,
            data,
            true,
            Some(on_key),
            thread,
        );
        ffi::emscripten_set_mousedown_callback_on_thread(
            canvas,
            data,
            true,
            Some(on_mouse),
            thread,
        );
        // A button let go off the canvas is still let go.
        ffi::emscripten_set_mouseup_callback_on_thread(
            ffi::TARGET_DOCUMENT,
            data,
            true,
            Some(on_mouse),
            thread,
        );
        ffi::emscripten_set_mousemove_callback_on_thread(
            canvas,
            data,
            true,
            Some(on_mouse),
            thread,
        );
        ffi::emscripten_set_mouseenter_callback_on_thread(
            canvas,
            data,
            true,
            Some(on_mouse),
            thread,
        );
        ffi::emscripten_set_mouseleave_callback_on_thread(
            canvas,
            data,
            true,
            Some(on_mouse),
            thread,
        );
        ffi::emscripten_set_contextmenu_callback_on_thread(
            canvas,
            data,
            true,
            Some(on_context_menu),
            thread,
        );
        ffi::emscripten_set_wheel_callback_on_thread(canvas, data, true, Some(on_wheel), thread);
        ffi::emscripten_set_resize_callback_on_thread(
            ffi::TARGET_WINDOW,
            data,
            false,
            Some(on_resize),
            thread,
        );
        ffi::emscripten_set_focus_callback_on_thread(
            ffi::TARGET_WINDOW,
            data,
            false,
            Some(on_focus),
            thread,
        );
        ffi::emscripten_set_blur_callback_on_thread(
            ffi::TARGET_WINDOW,
            data,
            false,
            Some(on_focus),
            thread,
        );
        ffi::emscripten_set_fullscreenchange_callback_on_thread(
            ffi::TARGET_DOCUMENT,
            data,
            false,
            Some(on_fullscreen),
            thread,
        );
        ffi::emscripten_set_pointerlockchange_callback_on_thread(
            ffi::TARGET_DOCUMENT,
            data,
            false,
            Some(on_pointer_lock),
            thread,
        );
        ffi::emscripten_set_visibilitychange_callback_on_thread(
            data,
            false,
            Some(on_visibility),
            thread,
        );
    }
}
