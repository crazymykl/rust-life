#![cfg_attr(all(test, feature = "unstable"), feature(test))]
#![cfg_attr(feature = "unstable", feature(portable_simd, coverage_attribute))]

#[cfg(all(test, feature = "unstable"))]
mod benchmarks;

mod bitboard;
mod lifeboard;

#[cfg(feature = "gui")]
mod gui;

/// Runs the GUI's windowed self-test, for the `tests/gui.rs` test binary: on
/// macOS `winit` requires the event loop on the main thread, so the windowed
/// code paths can only be exercised from a test binary whose `main` is the
/// event loop.
#[cfg(feature = "gui")]
pub use gui::gui_selftest;

/// Runs the GUI with a fixed, browser-appropriate configuration. The web build
/// has no command line (clap reads `std::env::args`, which is unsupported on
/// wasm), so its entry point calls this instead of [`run`].
#[cfg(all(feature = "gui", target_family = "wasm"))]
pub fn run_web() {
    // The page's canvas is a fixed CSS size (web/index.html); in device
    // pixels that is CSS size × the display's DPR, so seed a board that
    // exactly fills the canvas at `SCALE` px per cell — on a high-DPI display
    // a fixed 64×64 seed would be dead-padded to the canvas, leaving the
    // random field in a corner.
    const CANVAS_CSS_PX: f64 = 512.0;
    const SCALE: f64 = 8.0;
    let dpr = web_sys::window()
        .map(|w| w.device_pixel_ratio())
        .unwrap_or(1.0);
    let side = (CANVAS_CSS_PX * dpr / SCALE).round() as usize;
    // 60 steps per second, Conway's rule, no generation cap — left to the
    // user to pause (space/right-click) and edit (left-click) on screen.
    let brd = ScalarBitBoard::new(side, side).random();
    gui::run(brd, SCALE, 60, true, None, false);
}

/// The wasm entry point: `wasm-bindgen` calls `start` when the module is
/// loaded. It runs on the main thread (wasm is single-threaded here), which is
/// where `winit` and `wgpu` require it.
#[cfg(all(feature = "gui", target_family = "wasm"))]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
fn start() {
    run_web();
}

mod args;
mod board;
mod rules;

use args::{Alignment, Args, Backend, parse_args};
#[cfg(feature = "rayon")]
use bitboard::ParallelScalarBitBoard;
#[cfg(all(feature = "rayon", feature = "unstable"))]
use bitboard::ParallelSimdBitBoard;
use bitboard::ScalarBitBoard;
#[cfg(feature = "unstable")]
use bitboard::SimdBitBoard;
use board::Board;
use lifeboard::LifeBoard;
use std::str::FromStr;
use std::time::{Duration, Instant};

pub use rules::Rules;

pub const CLEAR: &str = "\x1b[H\x1b[2J";

pub fn run() {
    let args = parse_args();
    let cli_run_gens = args.generation_limit.or(args.generations.and(Some(0)));
    // The backend flag selects the concrete board type; the rest of the
    // program is generic over `B`, so there's no runtime indirection.
    match args.backend {
        Backend::Board => run_with(&args, make_board::<Board>(&args), cli_run_gens),
        Backend::BitBoard => run_with(&args, make_board::<ScalarBitBoard>(&args), cli_run_gens),
        #[cfg(feature = "rayon")]
        Backend::Parallel => run_with(
            &args,
            make_board::<ParallelScalarBitBoard>(&args),
            cli_run_gens,
        ),
        #[cfg(feature = "unstable")]
        Backend::Simd => run_with(&args, make_board::<SimdBitBoard>(&args), cli_run_gens),
        #[cfg(all(feature = "rayon", feature = "unstable"))]
        Backend::ParallelSimd => run_with(
            &args,
            make_board::<ParallelSimdBitBoard>(&args),
            cli_run_gens,
        ),
    }
}

/// Drives the GUI or CLI for the selected backend.
fn run_with<B: LifeBoard + 'static>(args: &Args, brd: B, cli_run_gens: Option<usize>) {
    #[cfg(feature = "gui")]
    if args.no_gui {
        cli(brd, args.ups, cli_run_gens);
    } else {
        gui::run(
            brd,
            args.scale,
            args.ups,
            args.generations.is_none() || args.generation_limit.is_some(),
            args.generation_limit,
            args.exit_on_finish,
        );
    }
    #[cfg(not(feature = "gui"))]
    cli(brd, args.ups, cli_run_gens);
}

fn make_board<B: LifeBoard + FromStr<Err: std::fmt::Debug>>(args: &Args) -> B {
    let mut brd = if let Some(template) = &args.template {
        let template = B::from_str(&template.to_string()).expect("failed to parse template");
        let (top, right, bottom, left) = if let Some(padding) = &args.padding {
            parse_padding(padding)
        } else {
            let vertical_padding = (args.rows - template.rows()) as isize;
            let horizontal_padding = (args.cols - template.cols()) as isize;

            alignment_padding(args.align, horizontal_padding, vertical_padding)
        };

        template
            .pad(top, right, bottom, left)
            .with_rules(&args.rules)
    } else {
        B::new(args.rows, args.cols)
            .with_rules(&args.rules)
            .random()
    };

    for _ in 0..args.generations.unwrap_or(0) {
        brd.step();
    }

    brd
}

fn parse_padding(padding: &[isize]) -> (isize, isize, isize, isize) {
    match *padding {
        [x] => (x, x, x, x),
        [vert, horiz] => (vert, horiz, vert, horiz),
        [t, horiz, b] => (t, horiz, b, horiz),
        [t, r, b, l] => (t, r, b, l),
        ref err => unreachable!("bad value for padding: '{err:?}'"),
    }
}

fn alignment_padding(
    align: Alignment,
    horizontal_padding: isize,
    vertical_padding: isize,
) -> (isize, isize, isize, isize) {
    let (top, bottom) = match align {
        Alignment::TopLeft | Alignment::Top | Alignment::TopRight => (0, vertical_padding),
        Alignment::Left | Alignment::Center | Alignment::Right => (
            vertical_padding / 2,
            vertical_padding / 2 + vertical_padding % 2,
        ),
        Alignment::BottomLeft | Alignment::Bottom | Alignment::BottomRight => (vertical_padding, 0),
    };
    let (left, right) = match align {
        Alignment::TopLeft | Alignment::Left | Alignment::BottomLeft => (0, horizontal_padding),
        Alignment::Top | Alignment::Center | Alignment::Bottom => (
            horizontal_padding / 2,
            horizontal_padding / 2 + horizontal_padding % 2,
        ),
        Alignment::TopRight | Alignment::Right | Alignment::BottomRight => (horizontal_padding, 0),
    };

    (top, right, bottom, left)
}

fn cli<B: LifeBoard>(mut brd: B, ups: u64, run_gens: Option<usize>) {
    if run_gens == Some(0) {
        println!("{brd}");
    } else {
        let frame_time: Duration = Duration::from_secs_f64(1.0 / ups as f64);
        let mut frame_start;

        while run_gens.is_none() || Some(brd.generation()) <= run_gens {
            frame_start = Instant::now();
            println!("{CLEAR}{brd}");
            brd.step();
            std::thread::sleep(
                frame_time.saturating_sub(Instant::now().duration_since(frame_start)),
            );
        }
    }
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Args::command().debug_assert();
}

#[test]
fn test_parse_padding() {
    assert_eq!(parse_padding(&[1]), (1, 1, 1, 1));
    assert_eq!(parse_padding(&[1, 2]), (1, 2, 1, 2));
    assert_eq!(parse_padding(&[1, 2, 3]), (1, 2, 3, 2));
    assert_eq!(parse_padding(&[1, 2, 3, 4]), (1, 2, 3, 4));
}

#[test]
#[should_panic = "bad value for padding: '[]'"]
fn test_parse_padding_invalid() {
    parse_padding(&[]);
}

#[test]
#[should_panic = "bad value for padding: '[1, 2, 3, 4, 5]'"]
fn test_parse_padding_invalid_2() {
    parse_padding(&[1, 2, 3, 4, 5]);
}

#[test]
fn test_alignment_padding() {
    assert_eq!(alignment_padding(Alignment::Top, 2, 2), (0, 1, 2, 1));
    assert_eq!(alignment_padding(Alignment::TopLeft, 2, 2), (0, 2, 2, 0));
    assert_eq!(alignment_padding(Alignment::TopRight, 2, 2), (0, 0, 2, 2));
    assert_eq!(alignment_padding(Alignment::Center, 2, 2), (1, 1, 1, 1));
    assert_eq!(alignment_padding(Alignment::Left, 2, 2), (1, 2, 1, 0));
    assert_eq!(alignment_padding(Alignment::Right, 2, 2), (1, 0, 1, 2));
    assert_eq!(alignment_padding(Alignment::Bottom, 2, 2), (2, 1, 0, 1));
    assert_eq!(alignment_padding(Alignment::BottomLeft, 2, 2), (2, 2, 0, 0));
    assert_eq!(
        alignment_padding(Alignment::BottomRight, 2, 2),
        (2, 0, 0, 2)
    );
}

#[cfg(test)]
mod backend_tests {
    use super::*;

    #[test]
    fn backends_agree() {
        // The two backends, started from the same template, must evolve
        // identically through the shared `LifeBoard` interface.
        let template = ".@.\n..@\n@@."; // a glider
        let mut board: Board = Board::from_str(template).unwrap();
        let mut bits: ScalarBitBoard = ScalarBitBoard::from_str(template).unwrap();
        for _ in 0..5 {
            board.step();
            bits.step();
        }
        assert_eq!(format!("{board}"), format!("{bits}"));
    }

    #[test]
    fn backends_agree_with_custom_rule() {
        // Both backends must also agree under a non-Conway rule, exercising
        // BitBoard's general per-count path against the reference Board.
        let rule = Rules::from_str("B368/S245").unwrap(); // Day & Night

        let mut board = Board::new(40, 40).with_rules(&rule).random();
        let mut bits = ScalarBitBoard::from(&board).with_rules(&rule);
        for _ in 0..6 {
            assert_eq!(
                format!("{board}"),
                bits.to_string(),
                "Day & Night mismatch at a generation"
            );
            board.step();
            bits.step();
        }
    }
}
