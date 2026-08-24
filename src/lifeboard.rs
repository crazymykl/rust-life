//! A behavioral interface shared by the board representations, so the GUI and
//! CLI can drive any of them (the `Vec<bool>` `Board` and the bit-packed
//! `BitBoard`) without knowing which one they got.
//!
//! The fast, allocation-free paths (`BitBoard::step`, and its `std::simd`
//! `SimdKernel` variant) stay as inherent methods on `BitBoard`; this trait
//! only covers the value-returning surface the application actually drives.

use std::fmt::Display;
use std::str::FromStr;

use crate::Rules;

pub const LIVE_CELL: char = '@';
pub const DEAD_CELL: char = '.';

/// A cellular board the application can run over.
pub trait LifeBoard: Display + FromStr + Eq {
    /// A fresh, all-dead board of the given size.
    fn new(rows: usize, cols: usize) -> Self;

    fn rows(&self) -> usize;
    fn cols(&self) -> usize;
    fn generation(&self) -> usize;

    #[allow(clippy::len_without_is_empty)]
    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.rows() * self.cols()
    }
    /// Number of live cells.
    #[allow(dead_code)]
    fn population(&self) -> usize;

    /// Advance one generation, returning the result (may allocate).
    fn next_generation(&self) -> Self;

    /// Advance one generation in place. The default allocates via
    // `next_generation`; `BitBoard` overrides it with the double-buffered,
    // allocation-free path.
    fn step(&mut self) {
        *self = self.next_generation();
    }

    /// Return a copy of this board that simulates under the given rule
    /// instead of its own. Cell layout is unchanged.
    fn with_rules(&self, rules: &Rules) -> Self;

    /// Flip a single cell. A no-op if the coordinate is out of bounds.
    #[cfg_attr(all(not(feature = "gui"), not(test)), allow(dead_code))]
    fn toggle(&self, x: usize, y: usize) -> Self;

    /// An all-dead board of the same size.
    #[cfg_attr(all(not(feature = "gui"), not(test)), allow(dead_code))]
    fn clear(&self) -> Self;

    /// A random board of the same size.
    fn random(&self) -> Self;

    /// Grow the board on each side by the given margins.
    fn pad(&self, top: isize, right: isize, bottom: isize, left: isize) -> Self;

    /// Visit every live/dead cell in row-major order.
    fn iter(&self) -> impl Iterator<Item = bool> + '_;
}
