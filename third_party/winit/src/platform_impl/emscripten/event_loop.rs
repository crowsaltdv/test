use std::cell::Cell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::marker::PhantomData;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Instant;

use crate::error::EventLoopError;
use crate::event::{self, StartCause};
use crate::event_loop::{self, ControlFlow, DeviceEvents};
use crate::window::{
    CustomCursor as RootCustomCursor, CustomCursorSource, Theme, WindowId as RootWindowId,
};

use super::{ffi, MonitorHandle, PlatformSpecificEventLoopAttributes, Queued, WindowId, SHARED};

pub struct EventLoop<T: 'static> {
    window_target: event_loop::ActiveEventLoop,
    user_events_sender: mpsc::Sender<T>,
    user_events_receiver: mpsc::Receiver<T>,
}

/// What a frame needs, kept for the life of the page: the browser calls [`frame`] with it.
struct Running<T: 'static> {
    event_loop: EventLoop<T>,
    /// The program's handler. It may borrow from the stack of `run`'s caller, which
    /// `emscripten_set_main_loop_arg` leaves in place: `run` never returns.
    handler: Box<dyn FnMut(event::Event<T>, &event_loop::ActiveEventLoop)>,
    started: bool,
    /// When the last frame that ran ended.
    waiting_since: Instant,
    finished: bool,
}

impl<T: 'static> EventLoop<T> {
    pub(crate) fn new(_: &PlatformSpecificEventLoopAttributes) -> Result<Self, EventLoopError> {
        let (user_events_sender, user_events_receiver) = mpsc::channel();
        Ok(Self {
            window_target: event_loop::ActiveEventLoop {
                p: ActiveEventLoop {
                    control_flow: Cell::new(ControlFlow::default()),
                    exit: Cell::new(false),
                },
                _marker: PhantomData,
            },
            user_events_sender,
            user_events_receiver,
        })
    }

    /// Hands the loop to the browser, which runs one iteration per animation frame. It does
    /// not return: Emscripten leaves `main` by an exception the page ignores, with the stack
    /// as it is, so what the handler borrows stays where it was. After `exit`, the frames
    /// stop and the page stays as the last one drew it.
    pub fn run<F>(self, event_handler: F) -> Result<(), EventLoopError>
    where
        F: FnMut(event::Event<T>, &event_loop::ActiveEventLoop),
    {
        let handler: Box<dyn FnMut(event::Event<T>, &event_loop::ActiveEventLoop) + '_> =
            Box::new(event_handler);
        // Safety: only the lifetime changes. The handler is called from `frame`, which the
        // browser calls while this function's caller is still on the stack (see above), and
        // never after `finished`.
        let handler: Box<dyn FnMut(event::Event<T>, &event_loop::ActiveEventLoop)> =
            unsafe { std::mem::transmute(handler) };
        let running = Box::into_raw(Box::new(Running {
            event_loop: self,
            handler,
            started: false,
            waiting_since: Instant::now(),
            finished: false,
        }));
        // Safety: `running` is leaked, so it outlives every call of `frame`; 0 frames per
        // second asks for the browser's animation frames.
        unsafe { ffi::emscripten_set_main_loop_arg(frame::<T>, running.cast(), 0, true) };
        Ok(())
    }

    pub fn window_target(&self) -> &event_loop::ActiveEventLoop {
        &self.window_target
    }

    pub fn create_proxy(&self) -> EventLoopProxy<T> {
        EventLoopProxy { user_events_sender: self.user_events_sender.clone() }
    }
}

/// One iteration of the loop, in winit's order: new events, the window's, the program's own,
/// a redraw, then about to wait.
extern "C" fn frame<T: 'static>(running: *mut c_void) {
    // Safety: the `Running` that `run` leaked, used by this thread's frames only, one at a time.
    let running = unsafe { &mut *running.cast::<Running<T>>() };
    if running.finished {
        return;
    }
    // The page's layout changes without an event too (a zoom, a panel opening).
    super::resized();

    let target = &running.event_loop.window_target;
    let now = Instant::now();
    let start = running.waiting_since;
    let woken = SHARED.wake.swap(false, Ordering::Relaxed) | SHARED.redraw.load(Ordering::Relaxed);
    let cause = if !running.started {
        StartCause::Init
    } else {
        match target.p.control_flow.get() {
            ControlFlow::Poll => StartCause::Poll,
            ControlFlow::Wait if woken => StartCause::WaitCancelled { start, requested_resume: None },
            ControlFlow::Wait => return,
            ControlFlow::WaitUntil(at) if now >= at => {
                StartCause::ResumeTimeReached { start, requested_resume: at }
            },
            ControlFlow::WaitUntil(at) if woken => {
                StartCause::WaitCancelled { start, requested_resume: Some(at) }
            },
            ControlFlow::WaitUntil(_) => return,
        }
    };

    let handler = &mut running.handler;
    handler(event::Event::NewEvents(cause), target);
    if !running.started {
        running.started = true;
        handler(event::Event::Resumed, target);
    }

    for queued in super::take_queue() {
        match queued {
            Queued::Window(event) => handler(
                event::Event::WindowEvent { window_id: RootWindowId(WindowId::CANVAS), event },
                target,
            ),
            Queued::Device(event) => handler(
                event::Event::DeviceEvent { device_id: event::DeviceId(super::DeviceId), event },
                target,
            ),
        }
    }

    while let Ok(event) = running.event_loop.user_events_receiver.try_recv() {
        handler(event::Event::UserEvent(event), target);
    }

    if SHARED.redraw.swap(false, Ordering::Relaxed) && SHARED.window.load(Ordering::Relaxed) {
        handler(
            event::Event::WindowEvent {
                window_id: RootWindowId(WindowId::CANVAS),
                event: event::WindowEvent::RedrawRequested,
            },
            target,
        );
    }

    handler(event::Event::AboutToWait, target);

    if target.p.exiting() {
        handler(event::Event::LoopExiting, target);
        running.finished = true;
        // Safety: stops the browser's calls of `frame`.
        unsafe { ffi::emscripten_cancel_main_loop() };
    }
    running.waiting_since = Instant::now();
}

pub struct EventLoopProxy<T: 'static> {
    user_events_sender: mpsc::Sender<T>,
}

impl<T> EventLoopProxy<T> {
    pub fn send_event(&self, event: T) -> Result<(), event_loop::EventLoopClosed<T>> {
        self.user_events_sender
            .send(event)
            .map_err(|mpsc::SendError(x)| event_loop::EventLoopClosed(x))?;
        SHARED.wake.store(true, Ordering::Relaxed);
        Ok(())
    }
}

impl<T> Clone for EventLoopProxy<T> {
    fn clone(&self) -> Self {
        Self { user_events_sender: self.user_events_sender.clone() }
    }
}

impl<T> Unpin for EventLoopProxy<T> {}

pub struct ActiveEventLoop {
    control_flow: Cell<ControlFlow>,
    exit: Cell<bool>,
}

impl ActiveEventLoop {
    pub fn create_custom_cursor(&self, source: CustomCursorSource) -> RootCustomCursor {
        let _ = source.inner;
        RootCustomCursor { inner: super::PlatformCustomCursor }
    }

    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        Some(MonitorHandle)
    }

    pub fn available_monitors(&self) -> VecDeque<MonitorHandle> {
        VecDeque::from([MonitorHandle])
    }

    #[inline]
    pub fn listen_device_events(&self, _allowed: DeviceEvents) {}

    #[cfg(feature = "rwh_05")]
    #[inline]
    pub fn raw_display_handle_rwh_05(&self) -> rwh_05::RawDisplayHandle {
        rwh_05::RawDisplayHandle::Web(rwh_05::WebDisplayHandle::empty())
    }

    #[inline]
    pub fn system_theme(&self) -> Option<Theme> {
        None
    }

    #[cfg(feature = "rwh_06")]
    #[inline]
    pub fn raw_display_handle_rwh_06(
        &self,
    ) -> Result<rwh_06::RawDisplayHandle, rwh_06::HandleError> {
        Ok(rwh_06::RawDisplayHandle::Web(rwh_06::WebDisplayHandle::new()))
    }

    pub fn set_control_flow(&self, control_flow: ControlFlow) {
        self.control_flow.set(control_flow)
    }

    pub fn control_flow(&self) -> ControlFlow {
        self.control_flow.get()
    }

    pub(crate) fn exit(&self) {
        self.exit.set(true);
    }

    pub(crate) fn exiting(&self) -> bool {
        self.exit.get()
    }

    pub(crate) fn owned_display_handle(&self) -> OwnedDisplayHandle {
        OwnedDisplayHandle
    }
}

#[derive(Clone)]
pub(crate) struct OwnedDisplayHandle;

impl OwnedDisplayHandle {
    #[cfg(feature = "rwh_05")]
    #[inline]
    pub fn raw_display_handle_rwh_05(&self) -> rwh_05::RawDisplayHandle {
        rwh_05::WebDisplayHandle::empty().into()
    }

    #[cfg(feature = "rwh_06")]
    #[inline]
    pub fn raw_display_handle_rwh_06(
        &self,
    ) -> Result<rwh_06::RawDisplayHandle, rwh_06::HandleError> {
        Ok(rwh_06::WebDisplayHandle::new().into())
    }
}
