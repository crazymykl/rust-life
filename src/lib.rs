#![cfg_attr(all(test, feature = "unstable"), feature(test))]

#[cfg(all(test, feature = "unstable"))]
mod benchmarks;

#[cfg(feature = "gui")]
mod gui;

#[cfg(all(feature = "test_mainthread", feature = "gui"))]
pub use gui::test_helper::EXAMPLES;

mod board;

use std::time::{Duration, Instant};

use board::Board;

pub const CLEAR: &str = "\x1b[H\x1b[2J";

mod args;

use args::{parse_args, Alignment, Args};

pub fn run() {
    let args = parse_args();
    let cli_run_gens = args.generation_limit.or(if args.generations.is_some() {
        Some(0)
    } else {
        None
    });
    let brd = make_board(&args);

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

fn make_board(args: &Args) -> Board {
    let mut brd = if let Some(template) = &args.template {
        let (top, right, bottom, left) = if let Some(padding) = &args.padding {
            parse_padding(padding)
        } else {
            let vertical_padding = (args.rows - template.rows()) as isize;
            let horizontal_padding = (args.cols - template.cols()) as isize;

            alignment_padding(args.align, horizontal_padding, vertical_padding)
        };

        template.pad(top, right, bottom, left)
    } else {
        Board::new(args.rows, args.cols).random()
    };

    for _ in 0..args.generations.unwrap_or(0) {
        brd = brd.next_generation();
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

fn cli(mut brd: Board, ups: u64, run_gens: Option<usize>) {
    if run_gens == Some(0) {
        println!("{brd}");
    } else {
        let frame_time: Duration = Duration::from_secs_f64(1.0 / ups as f64);
        let mut frame_start;

        while Some(brd.generation()) <= run_gens {
            frame_start = Instant::now();
            println!("{CLEAR}{brd}");
            brd = brd.next_generation();
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
