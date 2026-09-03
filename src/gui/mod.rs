//! GUI front-end, built directly on `winit` (events) and `wgpu` (rendering).
//!
//! The game state and all event *handling* live in the headless, GPU-free
//! [`State`], which can be constructed and driven without an `EventLoop` —
//! that's what the unit tests below exercise. The rendering lives in
//! [`renderer`](self::renderer) (the window renderer and the cell texture),
//! the shader source in board.wgsl, and the window and event-loop
//! plumbing in [`app`](self::app).

mod app;
mod renderer;
mod selftest;

use std::cmp::max;
use std::num::ParseFloatError;

use self::app::App;
use crate::lifeboard::LifeBoard;
use winit::dpi::PhysicalPosition;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};

const MIN_SCALE: f64 = 0.1;
const MAX_SCALE: f64 = 100.0;

/// Headless game state: the board and every piece of per-input state. No
/// window, no renderer, and no event loop are involved, so this can be built
/// and driven from unit tests.
struct State<B: LifeBoard> {
    brd: B,
    cursor: [f64; 2],
    scale: f64,
    running: bool,
    generation_limit: Option<usize>,
    exit_on_finish: bool,
    should_close: bool,
}

impl<B: LifeBoard> State<B> {
    fn new(
        brd: B,
        scale: f64,
        running: bool,
        generation_limit: Option<usize>,
        exit_on_finish: bool,
    ) -> Self {
        Self {
            brd,
            cursor: [0.0, 0.0],
            scale,
            running,
            generation_limit,
            exit_on_finish,
            should_close: false,
        }
    }

    fn scaled_cursor(&self) -> (usize, usize) {
        (
            (self.cursor[1] / self.scale).floor() as usize,
            (self.cursor[0] / self.scale).floor() as usize,
        )
    }

    fn board_size(&self) -> (u32, u32) {
        (self.brd.cols() as u32, self.brd.rows() as u32)
    }

    /// The board's size in pixels, at the current scale.
    fn board_px(&self) -> (f32, f32) {
        let scale = self.scale as f32;
        let (cols, rows) = self.board_size();
        (cols as f32 * scale, rows as f32 * scale)
    }

    fn cursor_moved(&mut self, physical: PhysicalPosition<f64>) {
        self.cursor = [physical.x, physical.y];
    }

    fn left_click(&mut self) {
        let (x, y) = self.scaled_cursor();
        self.brd = self.brd.toggle(x, y);
    }

    fn right_click(&mut self) {
        self.running = !self.running;
    }

    fn key_press(&mut self, key: &Key) {
        match key {
            Key::Named(NamedKey::Escape) => self.close_requested(),
            Key::Character(ch) => match ch.as_str() {
                " " => self.running = !self.running,
                "c" | "C" => self.brd = self.brd.clear(),
                "q" | "Q" => self.close_requested(),
                "r" | "R" => self.brd = self.brd.random(),
                "s" | "S" => self.brd.step(),
                _ => {}
            },
            _ => {}
        }
    }

    fn resized(&mut self, width: u32, height: u32) {
        let scale = self.scale;
        let (old_cols, old_rows) = (self.brd.cols(), self.brd.rows());
        let (cols, rows) = (
            max(old_cols, (width as f64 / scale).floor() as usize),
            max(old_rows, (height as f64 / scale).floor() as usize),
        );
        if cols != old_cols || rows != old_rows {
            self.brd = self
                .brd
                .pad(0, (cols - old_cols) as isize, (rows - old_rows) as isize, 0);
        }
    }

    fn update(&mut self) {
        if !self.running {
            return;
        }
        if Some(self.brd.generation()) == self.generation_limit {
            if self.exit_on_finish {
                self.close_requested();
            } else {
                self.running = false;
            }
        } else {
            self.brd.step();
        }
    }

    fn close_requested(&mut self) {
        // On the web there is no app to close — exiting the loop would just
        // freeze the GUI — so the browser tab (the only "window") is the
        // way out, and a close request is a no-op there.
        self.should_close = cfg!(not(target_family = "wasm"));
    }
}

pub(crate) fn valid_scale(s: &str) -> Result<f64, String> {
    match s.parse().map_err(|e: ParseFloatError| e.to_string())? {
        n @ MIN_SCALE..=MAX_SCALE => Ok(n),
        _ => Err(format!(
            "Scale must be between {MIN_SCALE} and {MAX_SCALE} (inclusive)"
        )),
    }
}

pub fn run<B: LifeBoard + 'static>(
    brd: B,
    scale: f64,
    ups: u64,
    init_running: bool,
    generation_limit: Option<usize>,
    exit_on_finish: bool,
) {
    let event_loop = EventLoop::new().expect("failed to create the winit event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let app = App::new(
        brd,
        scale,
        ups,
        init_running,
        generation_limit,
        exit_on_finish,
    );

    event_loop
        .run_app(app)
        .expect("event loop terminated with an error");
}

pub fn gui_selftest() {
    self::selftest::GuiSelfTest::new().run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;
    use std::str::FromStr;

    fn make_state() -> State<Board> {
        State::new(Board::new(3, 3), 4.0, true, Some(1), false)
    }

    #[test]
    fn cursor_move() {
        let mut s = make_state();
        assert_eq!(s.cursor, [0.0, 0.0]);
        s.cursor_moved(PhysicalPosition::new(9.0, 0.0));
        assert_eq!(s.cursor, [9.0, 0.0]);
    }

    #[test]
    fn click_cell_toggle() {
        let mut s = make_state();
        s.left_click();
        assert_eq!(s.brd.to_string(), "@..\n...\n...");
    }

    #[test]
    fn click_maps_cursor_to_cell() {
        // A cursor that is not a multiple of the scale exercises the
        // `floor` in `scaled_cursor`: (x, y) = (9.9, 7.9) at scale 4 maps to
        // (col 2, row 1), i.e. the middle row's rightmost cell.
        let mut s = make_state();
        s.cursor_moved(PhysicalPosition::new(9.9, 7.9));
        s.left_click();
        assert_eq!(s.brd.to_string(), "...\n..@\n...");
    }

    #[test]
    fn step_event() {
        let mut s = make_state();
        assert_eq!(s.brd.generation(), 0);
        s.key_press(&Key::Character("s".into()));
        assert_eq!(s.brd.generation(), 1);
    }

    #[test]
    fn clear_event() {
        let mut s = State::new(Board::from_str("@").unwrap(), 4.0, true, None, false);
        assert_eq!(s.brd.population(), 1);
        s.key_press(&Key::Character("c".into()));
        assert_eq!(s.brd.population(), 0);
    }

    #[test]
    fn randomize_event() {
        let mut s = State::new(Board::new(30, 30), 4.0, true, None, false);
        assert_eq!(s.brd.population(), 0);
        s.key_press(&Key::Character("r".into()));
        assert_ne!(s.brd.population(), 0);
    }

    #[test]
    fn quit_event() {
        let mut s = make_state();
        assert!(!s.should_close);
        s.key_press(&Key::Named(NamedKey::Escape));
        assert!(s.should_close);
    }

    #[test]
    fn q_key_quits() {
        let mut s = make_state();
        assert!(!s.should_close);
        s.key_press(&Key::Character("q".into()));
        assert!(s.should_close);
        let mut s = make_state();
        s.key_press(&Key::Character("Q".into()));
        assert!(s.should_close);
    }

    #[test]
    fn unhandled_key() {
        let mut s = make_state();
        let (brd, running) = (s.brd.to_string(), s.running);
        s.key_press(&Key::Named(NamedKey::CapsLock));
        assert_eq!(s.brd.to_string(), brd);
        assert_eq!(s.running, running);
    }

    #[test]
    fn toggle_running() {
        let mut s = make_state();
        assert!(s.running);
        s.key_press(&Key::Character(" ".into()));
        assert!(!s.running);
        s.right_click();
        assert!(s.running);
    }

    #[test]
    fn update_event() {
        let mut s = make_state();
        s.update();
        assert_eq!(s.brd.generation(), 1);
        assert!(s.running);
        s.update();
        assert_eq!(s.brd.generation(), 1);
        assert!(!s.running);
    }

    #[test]
    fn update_event_exits_on_finish() {
        let mut s = State::new(Board::new(3, 3), 4.0, true, Some(1), true);
        assert!(!s.should_close);
        s.update();
        assert_eq!(s.brd.generation(), 1);
        // Reaching the limit with `exit_on_finish` asks to close, without
        // pausing the run.
        s.update();
        assert!(s.should_close);
        assert!(s.running);
    }

    #[test]
    fn update_paused_is_noop() {
        let mut s = State::new(Board::new(3, 3), 4.0, false, None, false);
        assert!(!s.running);
        s.update();
        // A paused sim neither steps nor changes its running flag.
        assert_eq!(s.brd.generation(), 0);
        assert!(!s.running);
    }

    #[test]
    fn unhandled_character() {
        let mut s = make_state();
        let (brd, running) = (s.brd.to_string(), s.running);
        s.key_press(&Key::Character("a".into()));
        assert_eq!(s.brd.to_string(), brd);
        assert_eq!(s.running, running);
        assert!(!s.should_close);
    }

    #[test]
    fn resize_event() {
        let mut s = make_state();
        assert_eq!(s.brd.cols() * s.brd.rows(), 9);
        s.resized(40, 100);
        assert_eq!(s.brd.cols() * s.brd.rows(), 250);
        s.resized(40, 40);
        // we don't truncate the board if the window shrinks
        assert_eq!(s.brd.cols() * s.brd.rows(), 250);
    }

    #[test]
    fn board_px_matches_scale() {
        // A 4x3 board at scale 5 is 20 px wide and 15 px tall; the rect
        // uniform and window sizing both rely on this.
        let s = State::new(Board::new(3, 4), 5.0, true, None, false);
        assert_eq!(s.board_px(), (20.0, 15.0));
        let s = State::new(Board::new(3, 4), 0.5, true, None, false);
        assert_eq!(s.board_px(), (2.0, 1.5));
    }

    #[test]
    fn valid_scale_bounds() {
        assert_eq!(
            valid_scale("0"),
            Err(format!(
                "Scale must be between {MIN_SCALE} and {MAX_SCALE} (inclusive)"
            ))
        );
        // The range is inclusive at both endpoints.
        assert_eq!(valid_scale("0.1"), Ok(MIN_SCALE));
        assert_eq!(valid_scale("100"), Ok(MAX_SCALE));
        assert_eq!(valid_scale("1"), Ok(1.0));
        assert_eq!(
            valid_scale("9999"),
            Err(format!(
                "Scale must be between {MIN_SCALE} and {MAX_SCALE} (inclusive)"
            ))
        );
        assert_eq!(
            valid_scale("puppies"),
            Err("invalid float literal".to_string())
        );
    }
}
