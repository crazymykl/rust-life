use std::str::FromStr;

use piston_window::{
    Button, ButtonArgs, ButtonState, Event, Input, Key, Loop, Motion::MouseCursor, MouseButton,
    ResizeArgs, UpdateArgs, Window,
};

use crate::board::Board;

use super::GameState;

type Example<'a> = (&'a str, fn() -> ());

macro_rules! test_examples {
    ($($expr:expr),+ $(,)?) => [[$((stringify!($expr), $expr)),+]]
}

pub const EXAMPLES: &[Example] = &test_examples![
    test_cursor_move,
    test_click_cell_toggle,
    test_handle_step_event,
    test_handle_clear_event,
    test_handle_randomize_event,
    test_handle_quit_event,
    test_unhandled_button_event,
    test_toggle_running_event,
    test_update_event,
    test_resize_event,
];

fn make_gamestate(brd: Board) -> GameState {
    GameState::new(brd, 4.0, 1, true, Some(1), false)
}

fn mouse_move_event(x: f64, y: f64) -> Event {
    Input::Move(MouseCursor([x, y])).into()
}

fn button_event<T: Into<Button>>(button: T) -> Event {
    Input::Button(ButtonArgs {
        state: ButtonState::Press,
        button: button.into(),
        scancode: None,
    })
    .into()
}

fn update_event() -> Event {
    Loop::Update(UpdateArgs { dt: 0.0 }).into()
}

fn resize_event(x: f64, y: f64) -> Event {
    Input::Resize(ResizeArgs {
        window_size: [x, y],
        draw_size: [x as u32, y as u32],
    })
    .into()
}

fn test_cursor_move() {
    let mut gs = make_gamestate(Board::new(3, 3));

    assert_eq!(gs.cursor, [0.0, 0.0]);

    gs.handle_event(mouse_move_event(9.0, 0.0));

    assert_eq!(gs.cursor, [9.0, 0.0]);
}

fn test_click_cell_toggle() {
    let mut gs = make_gamestate(Board::new(3, 3));

    gs.handle_event(button_event(MouseButton::Left));

    assert_eq!(gs.brd.to_string(), "@..\n...\n...")
}

fn test_handle_step_event() {
    let mut gs = make_gamestate(Board::new(3, 3));

    assert_eq!(gs.brd.generation(), 0);

    gs.handle_event(button_event(Key::S));

    assert_eq!(gs.brd.generation(), 1);
}

fn test_handle_clear_event() {
    let mut gs = make_gamestate(Board::from_str("@").unwrap());

    assert_eq!(gs.brd.population(), 1);

    gs.handle_event(button_event(Key::C));

    assert_eq!(gs.brd.population(), 0);
}

fn test_handle_randomize_event() {
    let mut gs = make_gamestate(Board::new(30, 30));

    assert_eq!(gs.brd.population(), 0);

    gs.handle_event(button_event(Key::R));

    assert_ne!(gs.brd.population(), 0);
}

fn test_handle_quit_event() {
    let mut gs = make_gamestate(Board::new(3, 3));

    assert!(!gs.window.should_close());

    gs.handle_event(button_event(Key::Q));

    assert!(gs.window.should_close());
}

fn test_unhandled_button_event() {
    let mut gs = make_gamestate(Board::new(3, 3));
    let (brd, running) = (gs.brd.clone(), gs.running);

    gs.handle_event(button_event(Key::CapsLock));

    assert_eq!(gs.brd, brd);
    assert_eq!(gs.running, running);
}

fn test_toggle_running_event() {
    let mut gs = make_gamestate(Board::new(3, 3));

    assert!(gs.running);

    gs.handle_event(button_event(Key::Space));

    assert!(!gs.running);

    gs.handle_event(button_event(MouseButton::Right));

    assert!(gs.running);
}

fn test_update_event() {
    let mut gs: GameState = make_gamestate(Board::new(3, 3));

    gs.handle_event(update_event());

    assert_eq!(gs.brd.generation(), 1);
    assert!(gs.running);

    gs.handle_event(update_event());

    assert_eq!(gs.brd.generation(), 1);
    assert!(!gs.running);
}

fn test_resize_event() {
    let mut gs: GameState = make_gamestate(Board::new(3, 3));

    assert_eq!(gs.brd.len(), 9);

    gs.handle_event(resize_event(40.0, 100.0));

    assert_eq!(gs.brd.len(), 250);

    gs.handle_event(resize_event(40.0, 40.0));

    // we don't truncate the board if the window shrinks
    assert_eq!(gs.brd.len(), 250);
}
