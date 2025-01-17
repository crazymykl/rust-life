#![cfg_attr(all(test, feature = "unstable"), feature(test))]

#[cfg(all(test, feature = "unstable"))]
mod benchmarks;

mod board;

use std::{str::FromStr, time::Duration};

use board::Board;
use clap::Parser;

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
    #[arg(short, long, conflicts_with_all = ["rows", "cols"], value_parser = Board::from_str)]
    template: Option<Board>,

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
}

#[cfg(feature = "gui")]
fn valid_scale(s: &str) -> Result<f64, String> {
    const MIN_SCALE: f64 = 0.1;
    const MAX_SCALE: f64 = 10.0;

    match s.parse().map_err(|s| format!("{s}"))? {
        n @ 0.1..=10.0 => Ok(n),
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
        gui::main(&mut brd, args.scale, args.generations.is_none());
    } else {
        cli(&mut brd);
    }
    #[cfg(not(feature = "gui"))]
    cli(brd);
}

fn make_board(args: &Args) -> Board {
    let mut brd = if let Some(template) = &args.template {
        template.clone()
    } else {
        Board::new(args.rows, args.cols).random()
    };

    for _ in 0..args.generations.unwrap_or(0) {
        brd = brd.next_generation();
    }

    brd
}

fn cli(brd: &mut Board) {
    loop {
        println!("\x1b[H\x1b[2J{}", brd);
        std::thread::sleep(Duration::from_secs_f64(1.0 / 4.0));
        *brd = brd.next_generation();
    }
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Args::command().debug_assert();
}
