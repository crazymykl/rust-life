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

/// The renderer, or the in-flight build that will produce it. The two are
/// mutually exclusive, so they are a single state: `Pending` only exists on
/// the web (its device request runs in a `spawn_local` task and the result is
/// attached in [`App::proxy_wake_up`]); native builds the renderer
/// synchronously in `can_create_surfaces`, so it is always `Ready` there.
enum RendererState {
    /// The web's in-flight device build. The slot holds the result once the
    /// `spawn_local` task finishes; it is read and cleared in
    /// [`App::proxy_wake_up`]. Wasm is single-threaded (the build and the loop
    /// share the browser's main thread), so `Rc` + `Cell` suffices.
    #[cfg(target_family = "wasm")]
    Pending {
        slot: Rc<Cell<Result<Renderer, String>>>,
    },
    /// The attached renderer. Boxed (it is a bag of GPU handles) to keep the
    /// enum small; `get_renderer` dereferences it.
    Ready(Box<Renderer>),
}

/// The window and its [`Renderer`]. Both are created in `can_create_surfaces`.
struct Graphics {
    window: Arc<dyn Window>,
    renderer: RendererState,
}

/// A mutable reference to the [`Graphics`]' renderer, if it exists.
fn get_renderer(graphics: &mut Option<Graphics>) -> Option<&mut Renderer> {
    match &mut graphics.as_mut()?.renderer {
        #[cfg(target_family = "wasm")]
        RendererState::Pending { .. } => None,
        RendererState::Ready(renderer) => Some(&mut **renderer),
    }
}

/// The `ApplicationHandler`: owns the [`State`] and (once the surface is
/// ready) a [`Graphics`] — the window and its [`Renderer`] — paces the
/// simulation, and forwards `winit` window events into the headless
/// [`State`]. The window is built in [`App::can_create_surfaces`]; the
/// renderer is built there on native and attached in [`App::proxy_wake_up`]
/// on the web (see the `renderer` module for the two-phase build).
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
                Action::KeepRunning
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
                Action::KeepRunning
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == winit::event::ElementState::Pressed {
                    self.state.key_press(&event.logical_key);
                }
                Action::KeepRunning
            }
            WindowEvent::CloseRequested => {
                self.state.close_requested();
                Action::Exit
            }
            _ => Action::KeepRunning,
        }
    }

    fn request_redraw(&mut self) {
        if let Some(graphics) = &self.graphics {
            graphics.window.request_redraw();
        }
    }

    /// A mutable reference to the renderer, if it exists.
    fn renderer_mut(&mut self) -> Option<&mut Renderer> {
        get_renderer(&mut self.graphics)
    }

    /// Draw one frame if the window and renderer exist. `state` and the
    /// renderer are borrowed from disjoint fields, so they can be held at once
    /// (a `&mut self` helper like [`App::renderer_mut`] could not, as it
    /// borrows the whole `App`).
    fn draw_if_ready(&mut self) {
        let state = &self.state;
        if let Some(renderer) = get_renderer(&mut self.graphics) {
            renderer.draw(state);
        }
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
    /// Keep running; no event-loop side effect is needed.
    KeepRunning,
    /// Exit the event loop.
    Exit,
}

impl<B: LifeBoard + 'static> ApplicationHandler for App<B> {
    /// Create the window and, on native, the renderer. Every platform calls
    /// this once the render surface is safe to build. On the web the device
    /// request runs in the background and the renderer is attached in
    /// [`App::proxy_wake_up`].
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
        // Native builds the renderer synchronously (driving the async device
        // request inline with `block_on`), then seeds the cell texture and
        // rect uniform so the first frame is correct.
        #[cfg(not(target_family = "wasm"))]
        let renderer = {
            let unattached = Unattached::new_surface(&window, window.surface_size())
                .expect("failed to create a wgpu surface");
            let mut renderer = futures::executor::block_on(unattached.attach_device())
                .expect("failed to initialize the renderer");
            renderer.init_board_texture(&self.state);
            RendererState::Ready(Box::new(renderer))
        };
        // On the web the device request runs in the background; pass the
        // window's size explicitly, since `window.surface_size()` is not
        // populated until winit's async resize observer fires (after this
        // runs) and would otherwise yield a 0x0 surface.
        #[cfg(target_family = "wasm")]
        let renderer = pending_slot(&window, window_size, event_loop.create_proxy());
        self.graphics = Some(Graphics {
            window: window.clone(),
            renderer,
        });
        // Draw the first frame right away, instead of waiting for the first
        // `about_to_wait` pass (on the web it is skipped until the renderer
        // is attached).
        window.request_redraw();
    }

    /// The web's device build finished; attach the finished renderer (or fail)
    /// and draw the first frame. Native builds the renderer synchronously in
    /// `can_create_surfaces`, so this is never called there.
    #[cfg(target_family = "wasm")]
    fn proxy_wake_up(&mut self, _event_loop: &dyn ActiveEventLoop) {
        let Some(graphics) = self.graphics.as_mut() else {
            return;
        };
        let RendererState::Pending { slot } = &mut graphics.renderer else {
            return;
        };
        let Ok(mut renderer) = slot.replace(Err("already initialized".into())) else {
            return;
        };

        // The device build ran in the background, so any `SurfaceResized`
        // since `can_create_surfaces` was dropped — there was no renderer to
        // apply it to. Re-apply the window's current size: on a high-DPI
        // display it differs from the `board_px` size the surface was created
        // at, and the board must pad to keep `scale` px per cell.
        let size = graphics.window.surface_size();
        self.state.resized(size.width, size.height);
        renderer.reconfigure(size);

        // The surface now matches the window, so seed the cell texture and
        // the rect uniform from it for the first frame.
        renderer.init_board_texture(&self.state);

        graphics.renderer = RendererState::Ready(Box::new(renderer));
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
                let board_px = self.state.board_px();
                if let Some(renderer) = self.renderer_mut() {
                    renderer.reconfigure(size);
                    renderer.update_rect(board_px, size);
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
                self.draw_if_ready();
            }
            // The input events (and the close request) only mutate the
            // headless `State`, so `handle_event` interprets them; we carry
            // out the event-loop side effect it reports.
            _ => match self.handle_event(&event) {
                Action::KeepRunning => {}
                Action::Exit => event_loop.exit(),
            },
        }
    }
}

/// Build the web's `Pending` state: the surface, the result slot, and the
/// `spawn_local` task that fills it. A `block_on` is not an option, since the
/// request only resolves on browser microtasks; the task wakes the event
/// loop, and [`App::proxy_wake_up`] attaches the renderer.
#[cfg(target_family = "wasm")]
fn pending_slot(
    window: &Arc<dyn Window>,
    window_size: winit::dpi::PhysicalSize<u32>,
    proxy: winit::event_loop::EventLoopProxy,
) -> RendererState {
    let unattached =
        Unattached::new_surface(window, window_size).expect("failed to create a wgpu surface");
    let slot = Rc::new(Cell::new(Err("Uninitialized".into())));
    wasm_bindgen_futures::spawn_local({
        let slot = slot.clone();
        async move {
            slot.set(unattached.attach_device().await);
            proxy.wake_up();
        }
    });
    RendererState::Pending { slot }
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
        assert_eq!(action, Action::KeepRunning);
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
        assert_eq!(action, Action::KeepRunning);
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
        assert_eq!(action, Action::KeepRunning);
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
        assert_eq!(action, Action::KeepRunning);
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
        assert_eq!(action, Action::KeepRunning);
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
        assert_eq!(action, Action::KeepRunning);
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
        assert_eq!(action, Action::KeepRunning);
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
        assert_eq!(action, Action::KeepRunning);
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
        assert_eq!(action, Action::KeepRunning);
        assert_eq!(app.state.brd.to_string(), before);
    }
}
