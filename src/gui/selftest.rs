//! The windowed self-test, driven by the `tests/gui.rs` harness-less test
//! binary (see [`super::gui_selftest`]).

use std::sync::Arc;

use super::app::App;
use super::renderer::Unattached;
use crate::board::Board;
use crate::lifeboard::LifeBoard;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{WindowAttributes, WindowId};

/// Drives the windowed half of the GUI end to end, exercising the code paths
/// the headless tests can't reach: the pre-surface state (`about_to_wait` and
/// window events before the window and renderer exist), building the window
/// and renderer in `can_create_surfaces` (including its re-entrancy guard), a
/// real `SurfaceResized` (reconfigure the surface, pad the board, update the
/// rect, rebuild the cell texture), a `draw` with no cell texture yet, and
/// exiting through the `CloseRequested` path in `window_event`.
///
/// It can't run in a unit test on macOS: the `EventLoop` must be created and
/// run on the main thread, and `#[test]` bodies run on worker threads (and a
/// process can only host one event loop at a time). So `tests/gui.rs` — a
/// harness-less test binary whose `main` *is* the event loop — drives this.
/// It only *grows* the window, so it works on any real or virtual display.
pub(super) struct GuiSelfTest {
    app: App<Board>,
    frame: u32,
    // The frame on which the board first had the post-resize size.
    resized_at: Option<u32>,
    // The frame the `CloseRequested` was delivered, to hard-stop the loop if
    // the event loop fails to exit on it.
    closed_at: Option<u32>,
}

// A 200x200 window at the self-test's scale of 4 pads the board to 50x50.
const RESIZE_PX: u32 = 200;
const RESIZED_COLS: usize = RESIZE_PX as usize / 4;
const RESIZED_ROWS: usize = RESIZE_PX as usize / 4;
// At a 60 ups tick that is well over four seconds; plenty for the resize to
// land on a slow CI machine.
const MAX_FRAMES: u32 = 240;

impl GuiSelfTest {
    pub(super) fn new() -> Self {
        // Paused on purpose: the self-test asserts on structure, not on
        // evolution, so the simulation must not step.
        Self {
            app: App::new(Board::new(3, 3), 4.0, 60, false, None, false),
            frame: 0,
            resized_at: None,
            closed_at: None,
        }
    }

    /// `run_app` consumes the harness, so every assertion happens inside the
    /// loop (a failure panics, which fails this test binary).
    pub(super) fn run(self) {
        let event_loop = EventLoop::new().expect("failed to create the event loop");
        event_loop.set_control_flow(ControlFlow::Wait);
        event_loop
            .run_app(self)
            .expect("the self-test event loop terminated with an error");
    }

    /// The two "the self-test is already failing" guards, centralized so the
    /// never-taken-in-a-passing-run branches live inside a single
    /// `coverage(off)` function. Called every frame; on a healthy run it always
    /// returns.
    #[cfg_attr(feature = "unstable", coverage(off))]
    fn failure_guard(&self, cols: usize, rows: usize) {
        // The loop must terminate on its own once the app's `CloseRequested`
        // has been delivered.
        if let Some(closed_at) = self.closed_at
            && self.frame - closed_at >= 100
        {
            panic!("the event loop did not exit after `CloseRequested`");
        }
        // The board must be padded for the resize within `MAX_FRAMES`.
        if self.frame >= MAX_FRAMES {
            panic!(
                "the board was never padded for the resized window after {MAX_FRAMES} frames ({}x{})",
                cols, rows
            );
        }
    }
}

impl ApplicationHandler for GuiSelfTest {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        // An `App` that has not yet had its window built: on web and mobile
        // the loop can tick (and even receive window events) in this state,
        // so the app must tolerate it as no-ops. On desktop this state never
        // survives to an `about_to_wait` pass, so exercise it directly.
        // The `WindowId` is a dummy; the app ignores it.
        let mut pre_surface = App::new(Board::new(3, 3), 4.0, 60, false, None, false);
        pre_surface.about_to_wait(event_loop);
        pre_surface.window_event(
            event_loop,
            WindowId::from_raw(0),
            WindowEvent::SurfaceResized(PhysicalSize::new(0, 0)),
        );
        pre_surface.window_event(
            event_loop,
            WindowId::from_raw(0),
            WindowEvent::RedrawRequested,
        );

        // The real build happens on the first call; the second one must hit
        // the re-entrancy guard. (On desktop `can_create_surfaces` is only
        // emitted once, so this is the only place the guard is exercised.)
        self.app.can_create_surfaces(event_loop);
        self.app.can_create_surfaces(event_loop);

        // Draw one frame on a fresh window/renderer with no cell texture yet,
        // so `resize_if_needed`, `upload_cells`, and the no-cell guard in
        // `draw` all take their no-op paths.
        let scratch = Arc::from(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Life self-test scratch")
                        .with_surface_size(PhysicalSize::new(16, 16)),
                )
                .expect("failed to create the scratch window"),
        );
        {
            let unattached = Unattached::new_surface(&scratch, scratch.surface_size())
                .expect("failed to create the scratch surface");
            let mut renderer = futures::executor::block_on(unattached.attach_device())
                .expect("failed to initialize the scratch renderer");
            renderer.draw(self.app.state());
            // A zero-size reconfigure (the minimized-window guard) is a
            // no-op; a subsequent reconfigure still works.
            renderer.reconfigure(PhysicalSize::new(0, 0));
            renderer.reconfigure(PhysicalSize::new(16, 16));
            renderer.draw(self.app.state());
        }

        // Grow the app's window; the board pads to fit and the next draw
        // rebuilds the cell texture.
        let window = self.app.window().expect("the app window was never created");
        // A `None` return is the usual case: the request went to the display
        // system and the applied size arrives later as a `SurfaceResized`. The
        // frame wait below is where the resize is actually verified.
        let _ = window.request_surface_size(PhysicalSize::new(RESIZE_PX, RESIZE_PX).into());
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Only the app's window is forwarded; the scratch window's events
        // (an initial `SurfaceResized`, any redraws) are ignored.
        if Some(window_id) == self.app.window().map(|w| w.id()) {
            self.app.window_event(event_loop, window_id, event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.frame += 1;
        let (cols, rows) = (self.app.state().brd.cols(), self.app.state().brd.rows());
        // Hard stops in case the loop stalls; they only fire when the
        // self-test is already failing, so they can never run in a passing run
        // and are excluded from coverage.
        self.failure_guard(cols, rows);
        if self.resized_at.is_none() && cols >= RESIZED_COLS && rows >= RESIZED_ROWS {
            self.resized_at = Some(self.frame);
        }
        match self.resized_at {
            // The resize has landed; let a few frames draw it (which rebuilds
            // the cell texture) before closing through the app's own
            // `CloseRequested` path.
            Some(seen) if self.closed_at.is_none() && self.frame - seen >= 3 => {
                let window_id = self
                    .app
                    .window()
                    .map(|w| w.id())
                    .expect("no window to close");
                self.app
                    .window_event(event_loop, window_id, WindowEvent::CloseRequested);
                self.closed_at = Some(self.frame);
            }
            _ => self.app.about_to_wait(event_loop),
        }
    }
}
