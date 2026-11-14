//! The part of Emscripten's `emscripten.h` and `html5.h` this backend calls. The structs match
//! the headers field for field; a callback returning `true` has the browser's default action
//! prevented.
#![allow(dead_code)]

use std::ffi::{c_char, c_double, c_int, c_uint, c_ushort, c_void};

pub type Result = c_int;
pub const RESULT_SUCCESS: Result = 0;
/// The request waits for the next input event's handler (`deferUntilInEventHandler`).
pub const RESULT_DEFERRED: Result = 1;

/// `EMSCRIPTEN_EVENT_TARGET_DOCUMENT` and `_WINDOW`: not strings, but values of the pointer.
pub const TARGET_DOCUMENT: *const c_char = 1 as *const c_char;
pub const TARGET_WINDOW: *const c_char = 2 as *const c_char;
/// `EM_CALLBACK_THREAD_CONTEXT_CALLING_THREAD`: the callback runs on the thread that set it.
pub const CALLING_THREAD: usize = 2;

pub const EVENT_KEYDOWN: c_int = 2;
pub const EVENT_KEYUP: c_int = 3;
pub const EVENT_MOUSEDOWN: c_int = 5;
pub const EVENT_MOUSEUP: c_int = 6;
pub const EVENT_MOUSEMOVE: c_int = 8;
pub const EVENT_BLUR: c_int = 12;
pub const EVENT_FOCUS: c_int = 13;
pub const EVENT_MOUSEENTER: c_int = 33;
pub const EVENT_MOUSELEAVE: c_int = 34;

pub const DOM_DELTA_PIXEL: c_uint = 0;
pub const DOM_DELTA_LINE: c_uint = 1;
pub const DOM_DELTA_PAGE: c_uint = 2;

const SHORT: usize = 32;
const LONG: usize = 128;

#[repr(C)]
pub struct KeyboardEvent {
    pub timestamp: c_double,
    pub location: c_uint,
    pub ctrl_key: bool,
    pub shift_key: bool,
    pub alt_key: bool,
    pub meta_key: bool,
    pub repeat: bool,
    pub char_code: c_uint,
    pub key_code: c_uint,
    pub which: c_uint,
    pub key: [c_char; SHORT],
    pub code: [c_char; SHORT],
    pub char_value: [c_char; SHORT],
    pub locale: [c_char; SHORT],
}

#[repr(C)]
pub struct MouseEvent {
    pub timestamp: c_double,
    pub screen_x: c_int,
    pub screen_y: c_int,
    pub client_x: c_int,
    pub client_y: c_int,
    pub ctrl_key: bool,
    pub shift_key: bool,
    pub alt_key: bool,
    pub meta_key: bool,
    pub button: c_ushort,
    pub buttons: c_ushort,
    pub movement_x: c_int,
    pub movement_y: c_int,
    pub target_x: c_int,
    pub target_y: c_int,
    pub canvas_x: c_int,
    pub canvas_y: c_int,
    pub padding: c_int,
}

#[repr(C)]
pub struct WheelEvent {
    pub mouse: MouseEvent,
    pub delta_x: c_double,
    pub delta_y: c_double,
    pub delta_z: c_double,
    pub delta_mode: c_uint,
}

#[repr(C)]
pub struct UiEvent {
    pub detail: c_int,
    pub document_body_client_width: c_int,
    pub document_body_client_height: c_int,
    pub window_inner_width: c_int,
    pub window_inner_height: c_int,
    pub window_outer_width: c_int,
    pub window_outer_height: c_int,
    pub scroll_top: c_int,
    pub scroll_left: c_int,
}

#[repr(C)]
pub struct FocusEvent {
    pub node_name: [c_char; LONG],
    pub id: [c_char; LONG],
}

#[repr(C)]
pub struct FullscreenChangeEvent {
    pub is_fullscreen: bool,
    pub fullscreen_enabled: bool,
    pub node_name: [c_char; LONG],
    pub id: [c_char; LONG],
    pub element_width: c_int,
    pub element_height: c_int,
    pub screen_width: c_int,
    pub screen_height: c_int,
}

#[repr(C)]
pub struct PointerlockChangeEvent {
    pub is_active: bool,
    pub node_name: [c_char; LONG],
    pub id: [c_char; LONG],
}

#[repr(C)]
pub struct VisibilityChangeEvent {
    pub hidden: bool,
    pub visibility_state: c_int,
}

pub type KeyCallback = extern "C" fn(c_int, *const KeyboardEvent, *mut c_void) -> bool;
pub type MouseCallback = extern "C" fn(c_int, *const MouseEvent, *mut c_void) -> bool;
pub type WheelCallback = extern "C" fn(c_int, *const WheelEvent, *mut c_void) -> bool;
pub type UiCallback = extern "C" fn(c_int, *const UiEvent, *mut c_void) -> bool;
pub type FocusCallback = extern "C" fn(c_int, *const FocusEvent, *mut c_void) -> bool;
pub type FullscreenCallback =
    extern "C" fn(c_int, *const FullscreenChangeEvent, *mut c_void) -> bool;
pub type PointerlockCallback =
    extern "C" fn(c_int, *const PointerlockChangeEvent, *mut c_void) -> bool;
pub type VisibilityCallback =
    extern "C" fn(c_int, *const VisibilityChangeEvent, *mut c_void) -> bool;

extern "C" {
    pub fn emscripten_set_main_loop_arg(
        func: extern "C" fn(*mut c_void),
        arg: *mut c_void,
        fps: c_int,
        simulate_infinite_loop: bool,
    );
    pub fn emscripten_cancel_main_loop();
    pub fn emscripten_run_script(script: *const c_char);
    pub fn emscripten_get_device_pixel_ratio() -> c_double;
    pub fn emscripten_set_window_title(title: *const c_char);
    pub fn emscripten_get_screen_size(width: *mut c_int, height: *mut c_int);

    pub fn emscripten_set_canvas_element_size(
        target: *const c_char,
        width: c_int,
        height: c_int,
    ) -> Result;
    pub fn emscripten_get_element_css_size(
        target: *const c_char,
        width: *mut c_double,
        height: *mut c_double,
    ) -> Result;

    pub fn emscripten_request_pointerlock(target: *const c_char, defer: bool) -> Result;
    pub fn emscripten_exit_pointerlock() -> Result;
    pub fn emscripten_request_fullscreen(target: *const c_char, defer: bool) -> Result;
    pub fn emscripten_exit_fullscreen() -> Result;

    pub fn emscripten_set_keydown_callback_on_thread(
        target: *const c_char,
        user_data: *mut c_void,
        use_capture: bool,
        callback: Option<KeyCallback>,
        thread: usize,
    ) -> Result;
    pub fn emscripten_set_keyup_callback_on_thread(
        target: *const c_char,
        user_data: *mut c_void,
        use_capture: bool,
        callback: Option<KeyCallback>,
        thread: usize,
    ) -> Result;
    pub fn emscripten_set_mousedown_callback_on_thread(
        target: *const c_char,
        user_data: *mut c_void,
        use_capture: bool,
        callback: Option<MouseCallback>,
        thread: usize,
    ) -> Result;
    pub fn emscripten_set_mouseup_callback_on_thread(
        target: *const c_char,
        user_data: *mut c_void,
        use_capture: bool,
        callback: Option<MouseCallback>,
        thread: usize,
    ) -> Result;
    pub fn emscripten_set_mousemove_callback_on_thread(
        target: *const c_char,
        user_data: *mut c_void,
        use_capture: bool,
        callback: Option<MouseCallback>,
        thread: usize,
    ) -> Result;
    pub fn emscripten_set_mouseenter_callback_on_thread(
        target: *const c_char,
        user_data: *mut c_void,
        use_capture: bool,
        callback: Option<MouseCallback>,
        thread: usize,
    ) -> Result;
    pub fn emscripten_set_mouseleave_callback_on_thread(
        target: *const c_char,
        user_data: *mut c_void,
        use_capture: bool,
        callback: Option<MouseCallback>,
        thread: usize,
    ) -> Result;
    pub fn emscripten_set_contextmenu_callback_on_thread(
        target: *const c_char,
        user_data: *mut c_void,
        use_capture: bool,
        callback: Option<MouseCallback>,
        thread: usize,
    ) -> Result;
    pub fn emscripten_set_wheel_callback_on_thread(
        target: *const c_char,
        user_data: *mut c_void,
        use_capture: bool,
        callback: Option<WheelCallback>,
        thread: usize,
    ) -> Result;
    pub fn emscripten_set_resize_callback_on_thread(
        target: *const c_char,
        user_data: *mut c_void,
        use_capture: bool,
        callback: Option<UiCallback>,
        thread: usize,
    ) -> Result;
    pub fn emscripten_set_blur_callback_on_thread(
        target: *const c_char,
        user_data: *mut c_void,
        use_capture: bool,
        callback: Option<FocusCallback>,
        thread: usize,
    ) -> Result;
    pub fn emscripten_set_focus_callback_on_thread(
        target: *const c_char,
        user_data: *mut c_void,
        use_capture: bool,
        callback: Option<FocusCallback>,
        thread: usize,
    ) -> Result;
    pub fn emscripten_set_fullscreenchange_callback_on_thread(
        target: *const c_char,
        user_data: *mut c_void,
        use_capture: bool,
        callback: Option<FullscreenCallback>,
        thread: usize,
    ) -> Result;
    pub fn emscripten_set_pointerlockchange_callback_on_thread(
        target: *const c_char,
        user_data: *mut c_void,
        use_capture: bool,
        callback: Option<PointerlockCallback>,
        thread: usize,
    ) -> Result;
    pub fn emscripten_set_visibilitychange_callback_on_thread(
        user_data: *mut c_void,
        use_capture: bool,
        callback: Option<VisibilityCallback>,
        thread: usize,
    ) -> Result;
}
