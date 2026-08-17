//! A behavioral interface shared by the board representations, so the GUI and
//! CLI can drive any of them (the `Vec<bool>` `Board` and the bit-packed
//! `BitBoard`) without knowing which one they got.
//!
//! The fast, allocation-free paths (`BitBoard::step`/`step_simd`) stay as
//! inherent methods on `BitBoard`; this trait only covers the value-returning
//! surface the application actually drives.

use std::fmt::Display;
use std::str::FromStr;

/// A cellular board the application can run over.
pub trait LifeBoard: Display + FromStr + Eq {
    /// A fresh, all-dead board of the given size.
    fn new(rows: usize, cols: usize) -> Self;

    fn rows(&self) -> usize;
    fn cols(&self) -> usize;
    fn generation(&self) -> usize;
    /// Number of live cells.
    #[allow(dead_code)]
    fn population(&self) -> usize;

    /// Advance one generation, returning the result (may allocate).
    fn next_generation(&self) -> Self;

    /// Flip a single cell. A no-op if the coordinate is out of bounds.
    fn toggle(&self, x: usize, y: usize) -> Self;

    /// An all-dead board of the same size.
    fn clear(&self) -> Self;

    /// A random board of the same size.
    fn random(&self) -> Self;

    /// Grow the board on each side by the given margins.
    fn pad(&self, top: isize, right: isize, bottom: isize, left: isize) -> Self;

    /// Visit every live/dead cell in row-major order (padding excluded).
    fn for_each_cell(&self, f: impl FnMut(bool));
}
