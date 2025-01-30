#[cfg(feature = "gui")]
use crate::gui;
use crate::Board;
use clap::{Parser, ValueEnum};
use std::str::FromStr;

#[derive(ValueEnum, Copy, Clone, Debug)]
#[rustfmt::skip]
pub enum Alignment {
    TopLeft   , Top   , TopRight   ,
    Left      , Center, Right      ,
    BottomLeft, Bottom, BottomRight,
}

#[derive(Parser, Debug)]
#[command(version, about)]
pub(crate) struct Args {
    /// Number of columns of in the board
    #[arg(short, long, default_value_t = 640)]
    pub(crate) cols: usize,

    /// Number of rows of in the board
    #[arg(short, long, default_value_t = 400)]
    pub(crate) rows: usize,

    /// A board template string
    #[arg(short, long, value_parser = Board::from_str)]
    pub(crate) template: Option<Board>,

    /// Alignment of the template within the world
    #[arg(short, long, value_enum, default_value_t = Alignment::Center)]
    pub(crate) align: Alignment,

    /// Custom padding around template, takes 1 to 4 values (overrides alignment)
    #[arg(short, long, num_args = 1..=4, allow_negative_numbers = true, requires = "template", conflicts_with_all = ["align", "cols", "rows"])]
    pub(crate) padding: Option<Vec<isize>>,

    /// Number of generations to advance the template for the initial pattern
    #[arg(short, long)]
    pub(crate) generations: Option<usize>,

    /// Number of generations to display before stopping (runs forever if not given)
    #[arg(short = 'G', long)]
    pub(crate) generation_limit: Option<usize>,

    #[cfg(feature = "gui")]
    /// Scale factor (pixels per cell side)
    #[arg(short, long, default_value_t=2.0, value_parser = gui::valid_scale, conflicts_with = "no_gui")]
    pub(crate) scale: f64,

    #[cfg(feature = "gui")]
    /// Close GUI window after final generation
    #[arg(
        short = 'x',
        long,
        requires = "generation_limit",
        conflicts_with = "no_gui"
    )]
    pub(crate) exit_on_finish: bool,

    #[cfg(feature = "gui")]
    /// Disable GUI
    #[arg(long)]
    pub(crate) no_gui: bool,

    /// Updates per second (target)
    #[arg(short, long, default_value_t = 120)]
    pub(crate) ups: u64,
}

pub(crate) fn parse_args() -> Args {
    Args::parse()
}
