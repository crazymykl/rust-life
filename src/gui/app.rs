//! The `winit` side of the GUI: the window and event loop, and the
//! [`ApplicationHandler`] that paces the simulation and forwards window
//! events into the headless [`State`].

#[cfg(target_family = "wasm")]
use std::cell::Cell;
#[cfg(target_family = "wasm")]
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
#[cfg(not(target_family = "wasm"))]
use std::time::Instant;

use super::State;
use super::renderer::Renderer;
#[cfg(target_family = "wasm")]
use super::renderer::Unattached;
use crate::lifeboard::LifeBoard;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Window, WindowAttributes};

// winit's `ControlFlow::WaitUntil` is parameterized by the platform's clock:
// `std::time::Instant` on native targets and `web_time::Instant` on wasm
// (winit-core aliases it the same way), so pacing uses whichever applies.
#[cfg(target_family = "wasm")]
use web_time::Instant;

/// The slot the web's device build writes the finished (or failed) renderer
/// into; present only while the renderer is still building. Wasm is
/// single-threaded (the build is a `spawn_local` task on the browser's main
/// thread, the same one the loop runs on), so `Rc` + `Cell` suffices.
#[cfg(target_family = "wasm")]
#[derive(Clone)]
struct Pending(Rc<Cell<Option<Result<Renderer, String>>>>);

#[cfg(target_family = "wasm")]
impl Pending {
    fn new() -> Self {
        Self(Rc::new(Cell::new(None)))
    }

    /// The finished build, once the `spawn_local` task has completed.
    fn take(&self) -> Option<Result<Renderer, String>> {
        self.0.take()
    }

    /// Install the finished build. Called by the `spawn_local` task, which
    /// then wakes the loop to attach it.
    fn set(&self, result: Result<Renderer, String>) {
        self.0.set(Some(result));
    }
}

/// The window and its [`Renderer`]. The window is created in
/// `can_create_surfaces`.
///
/// On native the renderer is built synchronously there, so it is plain. On the
/// web its adapter and device request is async, so the renderer starts as
/// `None` and is attached in [`App::proxy_wake_up`].
#[cfg(not(target_family = "wasm"))]
struct Graphics {
    window: Arc<dyn Window>,
    renderer: Renderer,
}
#[cfg(target_family = "wasm")]
struct Graphics {
    window: Arc<dyn Window>,
    /// `None` while the device request is in flight (the `spawn_local` task
    /// writes its result into `pending`); `Some` once attached in
    /// [`App::proxy_wake_up`].
    renderer: Option<Renderer>,
    /// Present only while `renderer` is `None`.
    pending: Option<Pending>,
}

/// The `ApplicationHandler`: owns the [`State`] and (once the surface is
/// ready) a [`Graphics`] — the window and its [`Renderer`] — paces the
/// simulation, and forwards `winit` window events into the headless
/// [`State`]. The window is built in [`App::can_create_surfaces`]. On native
/// the renderer is built synchronously there too; on the web its adapter and
/// device request is async, so it finishes in the background and is attached
/// in [`App::proxy_wake_up`].
pub(super) struct App<B: LifeBoard> {
    state: State<B>,
    graphics: Option<Graphics>,
    tick: Duration,
    last_step: Instant,
}

impl<B: LifeBoard> App<B> {
    pub(super) fn new(
        brd: B,
        scale: f64,
        ups: u64,
        init_running: bool,
        generation_limit: Option<usize>,
        exit_on_finish: bool,
    ) -> Self {
        let tick = Duration::from_millis(1000 / ups.max(1));

        App {
            state: State::new(brd, scale, init_running, generation_limit, exit_on_finish),
            graphics: None,
            tick,
            last_step: Instant::now(),
        }
    }

    /// Apply the headless [`State`] transition for `event` and report the
    /// event-loop side effect (if any) the caller must perform.
    ///
    /// This is the pure, renderer-free half of event handling: it only
    /// mutates the [`State`], so it can be exercised from unit tests without
    /// a live `ActiveEventLoop` or window. [`App::window_event`] routes the
    /// input events through it and carries out the returned [`Action`].
    fn handle_event(&mut self, event: &WindowEvent) -> Action {
        match event {
            WindowEvent::PointerMoved { position, .. } => {
                self.state.cursor_moved(*position);
                Action::None
            }
            WindowEvent::PointerButton { state, button, .. } => {
                if *state == winit::event::ElementState::Pressed
                    && let winit::event::ButtonSource::Mouse(button) = button
                {
                    match button {
                        winit::event::MouseButton::Left => self.state.left_click(),
                        winit::event::MouseButton::Right => self.state.right_click(),
                        _ => {}
                    }
                }
                Action::None
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == winit::event::ElementState::Pressed {
                    self.state.key_press(&event.logical_key);
                }
                Action::None
            }
            WindowEvent::CloseRequested => {
                self.state.close_requested();
                Action::Exit
            }
            _ => Action::None,
        }
    }

    fn request_redraw(&mut self) {
        if let Some(graphics) = &self.graphics {
            graphics.window.request_redraw();
        }
    }

    /// The id of the window created in [`App::can_create_surfaces`], if any.
    /// Used by the GUI self-test to address the window.
    pub(super) fn window_id(&self) -> Option<winit::window::WindowId> {
        self.graphics.as_ref().map(|g| g.window.id())
    }

    /// The window created in [`App::can_create_surfaces`], if any. Used by the
    /// GUI self-test to resize it.
    pub(super) fn window(&self) -> Option<Arc<dyn Window>> {
        self.graphics.as_ref().map(|g| g.window.clone())
    }

    /// The game state. Used by the GUI self-test to read the board.
    pub(super) fn state(&self) -> &State<B> {
        &self.state
    }
}

/// The event-loop side effect a [`App::handle_event`] may request.
#[derive(PartialEq, Eq, Debug)]
enum Action {
    /// Nothing to do.
    None,
    /// Exit the event loop.
    Exit,
}

impl<B: LifeBoard + 'static> ApplicationHandler for App<B> {
    /// Create the window and the renderer. Every platform calls this once the
    /// render surface is safe to build (on desktop and web that's right after
    /// the initial `StartCause::Init`; winit's `resumed` is not emitted on
    /// desktop).
    ///
    /// On native the renderer is built synchronously (its adapter and device
    /// request is driven with `futures::executor::block_on`, which is safe
    /// here because the `wgpu` request resolves off-thread). On the web that
    /// would deadlock (the request resolves on browser microtasks, which can't
    /// run while a thread is parked), so the request is instead run as a
    /// `spawn_local` task and attached in [`App::proxy_wake_up`].
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        // Built once: the window (and the in-flight renderer) already exist.
        if self.graphics.is_some() {
            return;
        }
        let (width, height) = self.state.board_px();
        // At least 1 px per side, so a small board at a small scale still gets
        // a real window.
        let window_size =
            winit::dpi::PhysicalSize::new(width.max(1.0) as u32, height.max(1.0) as u32);
        let attributes = WindowAttributes::default()
            .with_title("Life")
            .with_surface_size(window_size);
        // On the web, insert the canvas into the page so it is visible.
        #[cfg(target_family = "wasm")]
        let attributes = attributes.with_platform_attributes(Box::new(
            winit::platform::web::WindowAttributesWeb::default().with_append(true),
        ));
        let window = Arc::from(
            event_loop
                .create_window(attributes)
                .expect("failed to create the winit window"),
        );
        #[cfg(not(target_family = "wasm"))]
        {
            // One instance backs both the surface and the adapter: each
            // `Instance` has its own object storage, and a surface is only
            // resolvable by the instance that created it. `for_window` drives
            // the async device request inline, then seeds the cell texture and
            // the rect uniform so the first frame is correct.
            let mut renderer =
                Renderer::for_window(&window).expect("failed to initialize the renderer");
            renderer.init_cell_texture(&self.state);
            let size = window.surface_size();
            renderer.update_rect(self.state.board_px(), (size.width, size.height));
            self.graphics = Some(Graphics {
                window: window.clone(),
                renderer,
            });
            // Draw the first frame right away, instead of waiting for the
            // first `about_to_wait` pass.
            window.request_redraw();
        }
        #[cfg(target_family = "wasm")]
        {
            // Pass the window's size explicitly: on the web `window.surface_size()`
            // is not populated until winit's async resize observer fires (after this
            // runs), so reading it here would yield 0x0 and the surface would be
            // configured at 0x0. `window_size` is the physical size the canvas's
            // drawing buffer is sized to.
            let unattached = Unattached::new_surface(&window, window_size)
                .expect("failed to create a wgpu surface");
            // The adapter and device request is async, so it runs as a
            // `spawn_local` task (a `block_on` here would deadlock). It writes
            // the finished renderer into this slot and wakes the loop, which
            // attaches it in `proxy_wake_up`.
            let pending = Pending::new();
            spawn_device_build(unattached, pending.clone(), event_loop.create_proxy());
            self.graphics = Some(Graphics {
                window,
                renderer: None,
                pending: Some(pending),
            });
        }
    }

    /// The web's device build finished; attach the finished renderer (or fail)
    /// and draw the first frame. Native builds the renderer synchronously in
    /// `can_create_surfaces`, so this is never called there.
    #[cfg(target_family = "wasm")]
    fn proxy_wake_up(&mut self, _event_loop: &dyn ActiveEventLoop) {
        let Some(pending) = self.graphics.as_mut().and_then(|g| g.pending.take()) else {
            return;
        };
        let Some(result) = pending.take() else {
            return;
        };
        let mut renderer = result.expect("failed to initialize the renderer");

        // The surface was created at the window's initial size, so seed the
        // cell texture and the rect uniform from it for the first frame.
        let size = renderer.surface_size();
        renderer.init_cell_texture(&self.state);
        renderer.update_rect(self.state.board_px(), size);

        // Publish the renderer and draw the first frame.
        let graphics = self
            .graphics
            .as_mut()
            .expect("the window exists while the build is pending");
        graphics.renderer = Some(renderer);
        graphics.window.request_redraw();
    }

    /// A `RedrawRequested` only arrives when a redraw is *requested*, so an
    /// idle window would otherwise never animate. Ask for one every pass and
    /// pace the loop to `ups` (winit's "run on demand" pattern); the loop
    /// still wakes early for input, so clicks and keys stay responsive.
    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.request_redraw();
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + self.tick));
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::SurfaceResized(size) => {
                // Pad the board for the new window size, then push the new
                // surface size and board/window pixel sizes to the renderer so
                // the shader keeps the board anchored at `scale` px per cell.
                self.state.resized(size.width, size.height);
                // The renderer exists together with the window on native, but
                // on the web it is `None` until the build finishes in
                // `proxy_wake_up`.
                #[cfg(not(target_family = "wasm"))]
                let renderer = self.graphics.as_mut().map(|g| &mut g.renderer);
                #[cfg(target_family = "wasm")]
                let renderer = self.graphics.as_mut().and_then(|g| g.renderer.as_mut());
                if let Some(renderer) = renderer {
                    renderer.reconfigure(size.width, size.height);
                    renderer.update_rect(self.state.board_px(), (size.width, size.height));
                }
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                // Advance the simulation at most once per `tick`, so input
                // events that wake the loop early don't speed it up.
                if Instant::now() >= self.last_step {
                    self.state.update();
                    self.last_step = Instant::now() + self.tick;
                    if self.state.should_close {
                        event_loop.exit();
                    }
                }
                // Only draw once the window and renderer exist; the renderer is
                // built in `can_create_surfaces` on native and attached in
                // `proxy_wake_up` on the web.
                #[cfg(not(target_family = "wasm"))]
                let renderer = self.graphics.as_mut().map(|g| &mut g.renderer);
                #[cfg(target_family = "wasm")]
                let renderer = self.graphics.as_mut().and_then(|g| g.renderer.as_mut());
                if let Some(renderer) = renderer {
                    renderer.draw(&self.state);
                }
            }
            // The input events (and the close request) only mutate the
            // headless `State`, so `handle_event` interprets them; we carry
            // out the event-loop side effect it reports.
            _ => match self.handle_event(&event) {
                Action::None => {}
                Action::Exit => event_loop.exit(),
            },
        }
    }
}

/// Run the async device build on the web, delivering the finished renderer (or
/// error) into `slot` and waking the event loop to attach it in
/// [`App::proxy_wake_up`].
///
/// This uses `spawn_local` rather than `futures::executor::block_on` because
/// the `wgpu` request resolves on browser microtasks, which can't run while a
/// thread is parked, so `block_on` would deadlock. (`spawn_local` also keeps
/// the build on the main thread, where the `!Send` `wgpu` types and the window
/// require it.)
#[cfg(target_family = "wasm")]
fn spawn_device_build(
    unattached: Unattached,
    slot: Pending,
    proxy: winit::event_loop::EventLoopProxy,
) {
    wasm_bindgen_futures::spawn_local(async move {
        slot.set(unattached.attach_device().await);
        proxy.wake_up();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;
    use winit::dpi::PhysicalPosition;
    use winit::event::{ButtonSource, ElementState, KeyEvent, MouseButton, PointerSource};
    use winit::keyboard::{Key, KeyLocation, NativeKeyCode, PhysicalKey};

    fn app() -> App<Board> {
        App::new(Board::new(3, 3), 4.0, 60, true, Some(1), false)
    }

    fn key_event(key: Key) -> KeyEvent {
        KeyEvent {
            physical_key: PhysicalKey::Unidentified(NativeKeyCode::Unidentified),
            logical_key: key.clone(),
            text: None,
            location: KeyLocation::Standard,
            state: ElementState::Pressed,
            repeat: false,
            text_with_all_modifiers: None,
            key_without_modifiers: key,
        }
    }

    #[test]
    fn pointer_moved_updates_cursor() {
        let mut app = app();
        let action = app.handle_event(&WindowEvent::PointerMoved {
            device_id: None,
            position: PhysicalPosition::new(9.0, 5.0),
            primary: true,
            source: PointerSource::Mouse,
        });
        assert_eq!(action, Action::None);
        assert_eq!(app.state.cursor, [9.0, 5.0]);
    }

    #[test]
    fn mouse_left_click_toggles_cell() {
        let mut app = app();
        app.state.cursor = [4.0, 4.0]; // center cell of a 3x3 at scale 4
        let action = app.handle_event(&WindowEvent::PointerButton {
            device_id: None,
            state: ElementState::Pressed,
            position: PhysicalPosition::new(4.0, 4.0),
            primary: true,
            button: ButtonSource::Mouse(MouseButton::Left),
        });
        assert_eq!(action, Action::None);
        assert_eq!(app.state.brd.to_string(), "...\n.@.\n...");
    }

    #[test]
    fn mouse_right_click_toggles_running() {
        let mut app = app();
        assert!(app.state.running);
        let action = app.handle_event(&WindowEvent::PointerButton {
            device_id: None,
            state: ElementState::Pressed,
            position: PhysicalPosition::new(0.0, 0.0),
            primary: true,
            button: ButtonSource::Mouse(MouseButton::Right),
        });
        assert_eq!(action, Action::None);
        assert!(!app.state.running);
    }

    #[test]
    fn mouse_release_is_ignored() {
        let mut app = app();
        let before = app.state.running;
        let action = app.handle_event(&WindowEvent::PointerButton {
            device_id: None,
            state: ElementState::Released,
            position: PhysicalPosition::new(0.0, 0.0),
            primary: true,
            button: ButtonSource::Mouse(MouseButton::Left),
        });
        assert_eq!(action, Action::None);
        assert_eq!(app.state.running, before);
    }

    #[test]
    fn non_mouse_button_is_ignored() {
        let mut app = app();
        app.state.cursor = [4.0, 4.0];
        let before = app.state.brd.to_string();
        let action = app.handle_event(&WindowEvent::PointerButton {
            device_id: None,
            state: ElementState::Pressed,
            position: PhysicalPosition::new(4.0, 4.0),
            primary: true,
            button: ButtonSource::Unknown(0),
        });
        assert_eq!(action, Action::None);
        assert_eq!(app.state.brd.to_string(), before);
    }

    #[test]
    fn mouse_middle_button_is_ignored() {
        // The `match` in `handle_event` only acts on Left and Right; the
        // other mouse buttons (Middle, Back, Forward) fall through.
        let mut app = app();
        app.state.cursor = [4.0, 4.0];
        let before = app.state.brd.to_string();
        let action = app.handle_event(&WindowEvent::PointerButton {
            device_id: None,
            state: ElementState::Pressed,
            position: PhysicalPosition::new(4.0, 4.0),
            primary: true,
            button: ButtonSource::Mouse(MouseButton::Middle),
        });
        assert_eq!(action, Action::None);
        assert_eq!(app.state.brd.to_string(), before);
    }

    #[test]
    fn keyboard_step_advances_generation() {
        let mut app = app();
        let action = app.handle_event(&WindowEvent::KeyboardInput {
            device_id: None,
            event: key_event(Key::Character("s".into())),
            is_synthetic: false,
        });
        assert_eq!(action, Action::None);
        assert_eq!(app.state.brd.generation(), 1);
    }

    #[test]
    fn keyboard_release_is_ignored() {
        let mut app = app();
        let mut released = key_event(Key::Character(" ".into()));
        released.state = ElementState::Released;
        let before = app.state.running;
        let action = app.handle_event(&WindowEvent::KeyboardInput {
            device_id: None,
            event: released,
            is_synthetic: false,
        });
        assert_eq!(action, Action::None);
        assert_eq!(app.state.running, before);
    }

    #[test]
    fn close_requested_exits() {
        let mut app = app();
        let action = app.handle_event(&WindowEvent::CloseRequested);
        assert_eq!(action, Action::Exit);
        assert!(app.state.should_close);
    }

    #[test]
    fn unrelated_event_is_ignored() {
        let mut app = app();
        let before = app.state.brd.to_string();
        let action = app.handle_event(&WindowEvent::Focused(true));
        assert_eq!(action, Action::None);
        assert_eq!(app.state.brd.to_string(), before);
    }
}
