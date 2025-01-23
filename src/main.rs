#![cfg_attr(all(test, feature = "unstable"), feature(test))]

#[cfg(all(test, feature = "unstable"))]
mod benchmarks;

mod board;

use std::{
    str::FromStr,
    time::{Duration, Instant},
};

use board::Board;
use clap::{Parser, ValueEnum};

#[derive(ValueEnum, Copy, Clone, Debug)]
enum HorizontalAlignment {
    Left,
    Center,
    Right,
}

#[derive(ValueEnum, Copy, Clone, Debug)]
enum VerticalAlignment {
    Top,
    Middle,
    Bottom,
}

#[derive(ValueEnum, Copy, Clone, Debug)]
#[rustfmt::skip]
enum Alignment {
    TopLeft   , Top   , TopRight   ,
    Left      , Center, Right      ,
    BottomLeft, Bottom, BottomRight,
}

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Number of columns of in the board
    #[arg(short, long, default_value_t = 640)]
    cols: usize,

    /// Number of rows of in the board
    #[arg(short, long, default_value_t = 400)]
    rows: usize,

    /// A board template string
    #[arg(short, long, value_parser = Board::from_str)]
    template: Option<Board>,

    /// Alignment of the template within the world
    #[arg(short, long, value_enum, default_value_t = Alignment::Center)]
    align: Alignment,

    /// Custom padding around template, takes 1 to 4 values (overrides alignment)
    #[arg(short, long, num_args = 1..=4, allow_negative_numbers = true, requires = "template", conflicts_with_all = ["align", "cols", "rows"])]
    padding: Option<Vec<isize>>,

    /// Number of generations to advance the initial pattern
    #[arg(short, long)]
    generations: Option<usize>,

    #[cfg(feature = "gui")]
    /// Scale factor (pixels per cell side)
    #[arg(short, long, default_value_t=2.0, value_parser = valid_scale)]
    scale: f64,

    #[cfg(feature = "gui")]
    /// Disable GUI
    #[arg(long)]
    no_gui: bool,

    /// Updates per second (target)
    #[arg(short, long, default_value_t = 120)]
    ups: u64,
}

#[cfg(feature = "gui")]
fn valid_scale(s: &str) -> Result<f64, String> {
    const MIN_SCALE: f64 = 0.1;
    const MAX_SCALE: f64 = 100.0;

    match s.parse().map_err(|s| format!("{s}"))? {
        n @ MIN_SCALE..=MAX_SCALE => Ok(n),
        _ => Err(format!(
            "Scale must be between {MIN_SCALE} and {MAX_SCALE} (inclusive)"
        )),
    }
}

#[cfg(feature = "gui")]
mod gui;

fn main() {
    let args = Args::parse();
    let mut brd = make_board(&args);

    #[cfg(feature = "gui")]
    if !args.no_gui {
        gui::main(&mut brd, args.scale, args.ups, args.generations.is_none());
    } else {
        cli(&mut brd, args.ups, args.generations.is_some());
    }
    #[cfg(not(feature = "gui"))]
    cli(&mut brd, args.ups, args.generations.is_some());
}

fn make_board(args: &Args) -> Board {
    let mut brd = if let Some(template) = &args.template {
        let (top, right, bottom, left) = if let Some(padding) = &args.padding {
            match padding[..] {
                [x] => (x, x, x, x),
                [vert, horiz] => (vert, horiz, vert, horiz),
                [t, horiz, b] => (t, horiz, b, horiz),
                [t, r, b, l] => (t, r, b, l),
                ref err => unreachable!("bad value for padding :'{err:?}'"),
            }
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

fn cli(brd: &mut Board, ups: u64, once: bool) {
    if once {
        println!("{brd}");
    } else {
        let frame_time = Duration::from_secs_f64(1.0 / ups as f64);
        let mut frame_start;
        loop {
            frame_start = Instant::now();
            clear();
            println!("{brd}");
            *brd = brd.next_generation();
            std::thread::sleep(frame_time - Instant::now().duration_since(frame_start));
        }
    }
}

fn clear() {
    print!("\x1b[H\x1b[2J");
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Args::command().debug_assert();
}
