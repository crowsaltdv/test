use std::collections::VecDeque;
use std::ffi::CString;
use std::sync::atomic::Ordering;

use crate::cursor::Cursor;
use crate::dpi::{PhysicalPosition, PhysicalSize, Position, Size};
use crate::platform_impl::Fullscreen;
use crate::window::ImePurpose;
use crate::{error, window};

use super::{ffi, ActiveEventLoop, MonitorHandle, OsError, WindowId, CANVAS, SHARED};

/// The page's canvas. A second one can't be made while the first lives.
pub struct Window {
    _private: (),
}

impl Window {
    pub(crate) fn new(
        _: &ActiveEventLoop,
        attrs: window::WindowAttributes,
    ) -> Result<Self, error::OsError> {
        if SHARED.window.swap(true, Ordering::Relaxed) {
            return Err(os_error!(OsError("a page has one window, its canvas")));
        }
        super::listen();
        // The page lays the canvas out; the size asked for is not the program's to choose.
        let _ = super::sync_size();
        let window = Self { _private: () };
        window.set_title(&attrs.title);
        window.set_cursor(attrs.cursor);
        if attrs.fullscreen.is_some() {
            window.set_fullscreen(attrs.fullscreen.map(Into::into));
        }
        SHARED.redraw.store(true, Ordering::Relaxed);
        Ok(window)
    }

    pub(crate) fn maybe_queue_on_main(&self, f: impl FnOnce(&Self) + Send + 'static) {
        f(self)
    }

    pub(crate) fn maybe_wait_on_main<R: Send>(&self, f: impl FnOnce(&Self) -> R + Send) -> R {
        f(self)
    }

    #[inline]
    pub fn id(&self) -> WindowId {
        WindowId::CANVAS
    }

    #[inline]
    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        Some(MonitorHandle)
    }

    #[inline]
    pub fn available_monitors(&self) -> VecDeque<MonitorHandle> {
        VecDeque::from([MonitorHandle])
    }

    #[inline]
    pub fn current_monitor(&self) -> Option<MonitorHandle> {
        Some(MonitorHandle)
    }

    #[inline]
    pub fn scale_factor(&self) -> f64 {
        super::scale_factor()
    }

    #[inline]
    pub fn request_redraw(&self) {
        SHARED.redraw.store(true, Ordering::Relaxed);
    }

    #[inline]
    pub fn pre_present_notify(&self) {}

    #[inline]
    pub fn reset_dead_keys(&self) {}

    #[inline]
    pub fn inner_position(&self) -> Result<PhysicalPosition<i32>, error::NotSupportedError> {
        Ok((0, 0).into())
    }

    #[inline]
    pub fn outer_position(&self) -> Result<PhysicalPosition<i32>, error::NotSupportedError> {
        Ok((0, 0).into())
    }

    #[inline]
    pub fn set_outer_position(&self, _: Position) {}

    #[inline]
    pub fn inner_size(&self) -> PhysicalSize<u32> {
        super::inner_size()
    }

    /// The page decides the canvas's size: the request is answered with the size it has.
    #[inline]
    pub fn request_inner_size(&self, _: Size) -> Option<PhysicalSize<u32>> {
        Some(super::inner_size())
    }

    #[inline]
    pub fn outer_size(&self) -> PhysicalSize<u32> {
        super::inner_size()
    }

    #[inline]
    pub fn set_min_inner_size(&self, _: Option<Size>) {}

    #[inline]
    pub fn set_max_inner_size(&self, _: Option<Size>) {}

    #[inline]
    pub fn title(&self) -> String {
        SHARED.title.lock().map(|t| t.clone()).unwrap_or_default()
    }

    /// The page's title.
    pub fn set_title(&self, title: &str) {
        if let Ok(mut kept) = SHARED.title.lock() {
            title.clone_into(&mut kept);
        }
        if let Ok(title) = CString::new(title) {
            // Safety: a NUL-terminated string, read before the call returns.
            unsafe { ffi::emscripten_set_window_title(title.as_ptr()) };
        }
    }

    #[inline]
    pub fn set_transparent(&self, _: bool) {}

    #[inline]
    pub fn set_blur(&self, _: bool) {}

    #[inline]
    pub fn set_visible(&self, _: bool) {}

    #[inline]
    pub fn is_visible(&self) -> Option<bool> {
        None
    }

    #[inline]
    pub fn resize_increments(&self) -> Option<PhysicalSize<u32>> {
        None
    }

    #[inline]
    pub fn set_resize_increments(&self, _: Option<Size>) {}

    #[inline]
    pub fn set_resizable(&self, _: bool) {}

    #[inline]
    pub fn is_resizable(&self) -> bool {
        true
    }

    #[inline]
    pub fn set_minimized(&self, _: bool) {}

    #[inline]
    pub fn is_minimized(&self) -> Option<bool> {
        None
    }

    #[inline]
    pub fn set_maximized(&self, _: bool) {}

    #[inline]
    pub fn is_maximized(&self) -> bool {
        false
    }

    /// The canvas over the whole screen. A browser grants it only from the handler of an
    /// input event, so the request waits for the next one (a key or a click).
    pub(crate) fn set_fullscreen(&self, fullscreen: Option<Fullscreen>) {
        // Safety: the selector is NUL-terminated.
        unsafe {
            if fullscreen.is_some() {
                ffi::emscripten_request_fullscreen(CANVAS.as_ptr(), true);
            } else if SHARED.fullscreen.load(Ordering::Relaxed) {
                ffi::emscripten_exit_fullscreen();
            }
        }
    }

    #[inline]
    pub(crate) fn fullscreen(&self) -> Option<Fullscreen> {
        SHARED.fullscreen.load(Ordering::Relaxed).then_some(Fullscreen::Borderless(None))
    }

    #[inline]
    pub fn set_decorations(&self, _: bool) {}

    #[inline]
    pub fn is_decorated(&self) -> bool {
        false
    }

    #[inline]
    pub fn set_window_level(&self, _: window::WindowLevel) {}

    #[inline]
    pub fn set_window_icon(&self, _: Option<crate::icon::Icon>) {}

    #[inline]
    pub fn set_ime_cursor_area(&self, _: Position, _: Size) {}

    #[inline]
    pub fn set_ime_allowed(&self, _: bool) {}

    #[inline]
    pub fn set_ime_purpose(&self, _: ImePurpose) {}

    #[inline]
    pub fn focus_window(&self) {}

    #[inline]
    pub fn request_user_attention(&self, _: Option<window::UserAttentionType>) {}

    /// The cursor over the canvas, by its CSS name.
    pub fn set_cursor(&self, cursor: Cursor) {
        if let Cursor::Icon(icon) = cursor {
            if let Ok(mut name) = SHARED.cursor_icon.lock() {
                icon.name().clone_into(&mut name);
            }
            apply_cursor();
        }
    }

    #[inline]
    pub fn set_cursor_position(&self, _: Position) -> Result<(), error::ExternalError> {
        Err(error::ExternalError::NotSupported(error::NotSupportedError::new()))
    }

    /// `Locked` is the page's pointer lock, which a browser grants only to a click or a key. The
    /// program is told it has it, and gets it as soon as the browser allows: now, when the page
    /// has a click or key from the last few seconds, else with the next click (which the program
    /// doesn't hear) or key. Until then the cursor stays visible. A page can't confine the pointer.
    pub fn set_cursor_grab(
        &self,
        mode: window::CursorGrabMode,
    ) -> Result<(), error::ExternalError> {
        match mode {
            window::CursorGrabMode::None => {
                SHARED.wants_lock.store(false, Ordering::Relaxed);
                if SHARED.pointer_locked.load(Ordering::Relaxed) {
                    // Safety: no arguments.
                    unsafe { ffi::emscripten_exit_pointerlock() };
                }
                Ok(())
            },
            window::CursorGrabMode::Confined => {
                Err(error::ExternalError::NotSupported(error::NotSupportedError::new()))
            },
            window::CursorGrabMode::Locked => {
                SHARED.wants_lock.store(true, Ordering::Relaxed);
                if !SHARED.pointer_locked.load(Ordering::Relaxed) {
                    super::request_lock();
                }
                // Success whatever the browser says: a refusal is a lock that comes with the
                // next click, and a browser with no lock at all still turns the view by the
                // pointer's movement.
                Ok(())
            },
        }
    }

    pub fn set_cursor_visible(&self, visible: bool) {
        SHARED.cursor_hidden.store(!visible, Ordering::Relaxed);
        apply_cursor();
    }

    #[inline]
    pub fn drag_window(&self) -> Result<(), error::ExternalError> {
        Err(error::ExternalError::NotSupported(error::NotSupportedError::new()))
    }

    #[inline]
    pub fn drag_resize_window(
        &self,
        _: window::ResizeDirection,
    ) -> Result<(), error::ExternalError> {
        Err(error::ExternalError::NotSupported(error::NotSupportedError::new()))
    }

    #[inline]
    pub fn show_window_menu(&self, _: Position) {}

    #[inline]
    pub fn set_cursor_hittest(&self, _: bool) -> Result<(), error::ExternalError> {
        Err(error::ExternalError::NotSupported(error::NotSupportedError::new()))
    }

    #[cfg(feature = "rwh_04")]
    #[inline]
    pub fn raw_window_handle_rwh_04(&self) -> rwh_04::RawWindowHandle {
        let mut handle = rwh_04::WebHandle::empty();
        handle.id = 1;
        rwh_04::RawWindowHandle::Web(handle)
    }

    #[cfg(feature = "rwh_05")]
    #[inline]
    pub fn raw_window_handle_rwh_05(&self) -> rwh_05::RawWindowHandle {
        let mut handle = rwh_05::WebWindowHandle::empty();
        handle.id = 1;
        rwh_05::RawWindowHandle::Web(handle)
    }

    #[cfg(feature = "rwh_05")]
    #[inline]
    pub fn raw_display_handle_rwh_05(&self) -> rwh_05::RawDisplayHandle {
        rwh_05::RawDisplayHandle::Web(rwh_05::WebDisplayHandle::empty())
    }

    /// A web handle, as wgpu's EGL backend expects under Emscripten, whose EGL draws on the
    /// page's canvas whatever the id.
    #[cfg(feature = "rwh_06")]
    #[inline]
    pub fn raw_window_handle_rwh_06(&self) -> Result<rwh_06::RawWindowHandle, rwh_06::HandleError> {
        Ok(rwh_06::RawWindowHandle::Web(rwh_06::WebWindowHandle::new(1)))
    }

    #[cfg(feature = "rwh_06")]
    #[inline]
    pub fn raw_display_handle_rwh_06(
        &self,
    ) -> Result<rwh_06::RawDisplayHandle, rwh_06::HandleError> {
        Ok(rwh_06::RawDisplayHandle::Web(rwh_06::WebDisplayHandle::new()))
    }

    #[inline]
    pub fn set_enabled_buttons(&self, _: window::WindowButtons) {}

    #[inline]
    pub fn enabled_buttons(&self) -> window::WindowButtons {
        window::WindowButtons::all()
    }

    #[inline]
    pub fn theme(&self) -> Option<window::Theme> {
        None
    }

    #[inline]
    pub fn has_focus(&self) -> bool {
        SHARED.focused.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn set_theme(&self, _: Option<window::Theme>) {}

    #[inline]
    pub fn set_content_protected(&self, _: bool) {}
}

impl Drop for Window {
    fn drop(&mut self) {
        SHARED.wants_lock.store(false, Ordering::Relaxed);
        SHARED.window.store(false, Ordering::Relaxed);
    }
}

/// The canvas's cursor: none while the program hid it and the browser holds the pointer lock,
/// else the one the program chose.
pub(crate) fn apply_cursor() {
    let hidden = SHARED.cursor_hidden.load(Ordering::Relaxed)
        && SHARED.pointer_locked.load(Ordering::Relaxed);
    if hidden {
        set_canvas_cursor("none");
    } else {
        let name = SHARED.cursor_icon.lock().map(|n| n.clone()).unwrap_or_default();
        set_canvas_cursor(if name.is_empty() { "default" } else { &name });
    }
}

/// Sets the canvas's CSS cursor. `name` is one of winit's cursor names or `none`: letters and
/// hyphens only, so it needs no quoting.
fn set_canvas_cursor(name: &str) {
    if name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') {
        super::run_script(&format!(
            "if(typeof document!=='undefined'){{var c=document.getElementById('canvas');if(c)c.\
             style.cursor='{name}';}}"
        ));
    }
}
