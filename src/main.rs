#![cfg_attr(all(test, feature = "unstable"), feature(test))]

#[cfg(all(test, feature = "unstable"))]
mod benchmarks;

mod board;

use std::time::Duration;

use board::Board;
use clap::Parser;

const MIN_SCALE: f64 = 0.1;
const MAX_SCALE: f64 = 10.0;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Number of columns of in the board
    #[arg(short, long, default_value_t = 640)]
    cols: u32,

    /// Number of rows of in the board
    #[arg(short, long, default_value_t = 400)]
    rows: u32,

    #[cfg(feature = "gui")]
    /// Scale factor (pixels per cell side)
    #[arg(short, long, default_value_t=2.0, value_parser = valid_scale)]
    scale: f64,

    #[cfg(feature = "gui")]
    /// Disable GUI
    #[arg(long)]
    no_gui: bool,
}

fn valid_scale(s: &str) -> Result<f64, String> {
    match s.parse().map_err(|s| format!("{s}"))? {
        n @ 0.1..=10.0 => Ok(n),
        _ => Err(format!(
            "Scale must be between {MIN_SCALE} and {MAX_SCALE} (inclusive)"
        )),
    }
}

#[cfg(all(not(test), feature = "gui"))]
mod gui;

#[cfg(not(test))]
fn main() {
    let args = Args::parse();

    #[cfg(feature = "gui")]
    if !args.no_gui {
        gui::main(args);
    } else {
        cli(args);
    }
    #[cfg(not(feature = "gui"))]
    cli(args);
}

fn cli(args: Args) {
    let mut brd = Board::new(args.rows as usize, args.cols as usize).random();
    loop {
        println!("\x1b[H\x1b[2J{}", brd);
        std::thread::sleep(Duration::from_secs_f64(1.0 / 4.0));
        brd = brd.next_generation();
    }
}
