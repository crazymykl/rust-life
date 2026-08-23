use crate::Rules;
#[cfg(feature = "gui")]
use crate::gui;
use clap::{Parser, ValueEnum};
use std::str::FromStr;

#[derive(ValueEnum, Copy, Clone, Debug)]
#[rustfmt::skip]
pub enum Alignment {
    TopLeft   , Top   , TopRight   ,
    Left      , Center, Right      ,
    BottomLeft, Bottom, BottomRight,
}

#[derive(ValueEnum, Copy, Clone, Debug)]
pub enum Backend {
    /// The `Vec<bool>` board (rayon-parallel so long as the feature is on).
    #[value(name = "board")]
    Board,

    /// The bit-packed `Vec<u64>` board (word-level, allocation-free).
    #[value(name = "bitboard")]
    BitBoard,

    /// The rayon row-parallel bit-packed board (`rayon` feature).
    #[cfg(feature = "rayon")]
    #[value(name = "parallel")]
    Parallel,

    /// The `std::simd` bit-packed board (two words at once, `unstable` only).
    #[cfg(feature = "unstable")]
    #[value(name = "simd")]
    Simd,

    /// The rayon row-parallel `std::simd` bit-packed board (`rayon` + `unstable`).
    #[cfg(all(feature = "rayon", feature = "unstable"))]
    #[value(name = "parallel-simd")]
    ParallelSimd,
}

impl std::fmt::Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Backend::Board => write!(f, "board"),
            Backend::BitBoard => write!(f, "bitboard"),
            #[cfg(feature = "rayon")]
            Backend::Parallel => write!(f, "parallel"),
            #[cfg(feature = "unstable")]
            Backend::Simd => write!(f, "simd"),
            #[cfg(all(feature = "rayon", feature = "unstable"))]
            Backend::ParallelSimd => write!(f, "parallel-simd"),
        }
    }
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
    #[arg(short, long)]
    pub(crate) template: Option<String>,

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

    /// Board representation to simulate
    #[arg(short, long, default_value_t = Backend::Board)]
    pub(crate) backend: Backend,

    /// Custom neighborhood rule in Golly form (e.g. `B368/S245` for Day &
    /// Night). Defaults to Conway's `B3/S23`.
    #[arg(long, default_value = "B3/S23", value_parser = Rules::from_str)]
    pub(crate) rules: Rules,
}

pub(crate) fn parse_args() -> Args {
    Args::parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_display() {
        assert_eq!(Backend::Board.to_string(), "board");
        assert_eq!(Backend::BitBoard.to_string(), "bitboard");
        #[cfg(feature = "rayon")]
        assert_eq!(Backend::Parallel.to_string(), "parallel");
        #[cfg(feature = "unstable")]
        assert_eq!(Backend::Simd.to_string(), "simd");
        #[cfg(all(feature = "rayon", feature = "unstable"))]
        assert_eq!(Backend::ParallelSimd.to_string(), "parallel-simd");
    }
}
