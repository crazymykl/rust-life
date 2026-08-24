//! A bit-packed representation of the board, using one 64-bit word per
//! 64 cells instead of one byte per cell.
//!
//! This is a prototype meant to be benchmarked against the `Vec<bool>` based
//! `Board` in `board.rs`. It demonstrates two optimizations over the naive
//! per-cell approach:
//!
//! * **Double buffering** — `step(&mut self)` reuses two word buffers and
//!   swaps them with `std::mem::take`, so advancing a generation allocates
//!   nothing.
//! * **Word-level, branchless neighbor counting** — instead of calling a
//!   per-cell function with per-edge branches, the 3x3 neighborhood is built
//!   as eight neighbor bitboards and reduced to the (odd, even) carry bits,
//!   then the rule is applied in a handful of bitwise ops with no per-cell
//!   loop: Conway's B3/S23 is one expression, other rules one per
//!   live-neighbor count.
//!
//! A `std::simd` variant (`SimdKernel`, in `kernel/simd.rs`) runs the same
//! formula on two adjacent words at once with `u64x2`. Kernels are selected at
//! the type level — `BitBoard` is generic over a `Kernel` — and the SIMD one is
//! gated behind the `unstable` feature.

use crate::Rules;
use crate::board::Board;
use crate::lifeboard::{DEAD_CELL, LIVE_CELL, LifeBoard};
use std::fmt::{self, Write};
use std::str::FromStr;

mod kernel;

#[cfg(feature = "rayon")]
pub(crate) use kernel::ParallelScalarKernel;
#[cfg(all(feature = "rayon", feature = "unstable"))]
pub(crate) use kernel::ParallelSimdKernel;
pub(crate) use kernel::ScalarKernel;
#[cfg(feature = "unstable")]
pub(crate) use kernel::SimdKernel;
use kernel::{Kernel, StepCtx};

const BITS: usize = 64;

#[derive(Clone, Debug)]
pub(crate) struct BitBoard<K: Kernel = ScalarKernel> {
    // Double buffer: the current generation's cells.
    current: Vec<u64>,
    // Scratch buffer for the next generation; swapped with `current` on step.
    next: Vec<u64>,
    words_per_row: usize,
    rows: usize,
    cols: usize,
    // The rule to apply. When it's Conway's B3/S23 the branchless fast path
    // runs; otherwise the general per-neighbor-count path does.
    rules: Rules,
    generation: usize,
    // The next-generation kernel to run on `step` (scalar, or the `std::simd`
    // pair kernel under `unstable`). A zero-sized type parameter, so it costs
    // nothing at runtime — only the chosen algorithm.
    kernel: K,
}

/// The default bit-packed board: the word-by-word, allocation-free `step`
/// (`ScalarKernel`). The `--backend bitboard` flag builds this one.
pub type ScalarBitBoard = BitBoard<ScalarKernel>;

/// The `std::simd` bit-packed board: the two-words-at-once `step`
/// (`SimdKernel`), available only under the `unstable` feature.
/// The `--backend simd` flag builds it.
#[cfg(feature = "unstable")]
pub type SimdBitBoard = BitBoard<SimdKernel>;

/// The rayon row-parallel bit-packed board (`--backend parallel`).
#[cfg(feature = "rayon")]
pub type ParallelScalarBitBoard = BitBoard<ParallelScalarKernel>;

/// The rayon row-parallel `std::simd` bit-packed board (`--backend parallel-simd`).
#[cfg(all(feature = "rayon", feature = "unstable"))]
pub type ParallelSimdBitBoard = BitBoard<ParallelSimdKernel>;

impl<K: Kernel> BitBoard<K> {
    fn rows(&self) -> usize {
        self.rows
    }

    fn cols(&self) -> usize {
        self.cols
    }

    fn generation(&self) -> usize {
        self.generation
    }
}

impl<K: Kernel> LifeBoard for BitBoard<K> {
    fn new(rows: usize, cols: usize) -> Self {
        let words_per_row = cols.div_ceil(BITS);
        let words = vec![0u64; rows * words_per_row];
        BitBoard {
            current: words.clone(),
            next: words,
            words_per_row,
            rows,
            cols,
            rules: Rules::default(),
            generation: 0,
            kernel: K::default(),
        }
    }

    fn rows(&self) -> usize {
        self.rows()
    }

    fn cols(&self) -> usize {
        self.cols()
    }

    fn generation(&self) -> usize {
        self.generation()
    }

    /// Advance one generation **in place**, reusing the two word buffers
    /// (no allocation). This is the double-buffered, word-level, bit-parallel
    /// path.
    fn population(&self) -> usize {
        // Only the low `cols` bits of the final word of each row are valid;
        // zero out the padding bits before counting.
        let pad = self.words_per_row * BITS - self.cols;
        let last_word_mask = !0u64 >> pad;
        let per_row = self.words_per_row;

        self.current
            .iter()
            .enumerate()
            .map(|(i, &w)| {
                if i % per_row == per_row - 1 {
                    (w & last_word_mask).count_ones()
                } else {
                    w.count_ones()
                }
            })
            .sum::<u32>() as usize
    }

    /// The double-buffered, allocation-free fast path.
    fn step(&mut self) {
        let current = std::mem::take(&mut self.current);
        let mut next = std::mem::take(&mut self.next);
        self.kernel.compute(
            &current,
            &mut next,
            &StepCtx {
                rules: &self.rules,
                rows: self.rows,
                words_per_row: self.words_per_row,
                cols: self.cols,
            },
        );
        self.current = next;
        self.next = current;
        self.generation += 1;
    }

    /// Compute a brand-new generation (allocates). Kept for the cross-check
    /// tests that expect `let next = bits.next_generation();`.
    #[allow(dead_code)]
    fn next_generation(&self) -> Self {
        let mut b = self.clone();
        b.step();
        b
    }

    fn toggle(&self, x: usize, y: usize) -> Self {
        let mut b = self.clone();
        if x < self.rows && y < self.cols {
            b.current[x * self.words_per_row + y / BITS] ^= 1u64 << (y % BITS);
        }
        b
    }

    fn clear(&self) -> Self {
        Self::new(self.rows, self.cols).with_rules(&self.rules)
    }

    fn random(&self) -> Self {
        use rand::{RngExt, distr::StandardUniform, rng};
        let len = self.current.len();
        let mut current = vec![0u64; len];
        let mut it = rng().sample_iter(&StandardUniform);
        for w in current.iter_mut() {
            *w = it.next().unwrap();
        }
        BitBoard {
            current,
            next: vec![0u64; len],
            words_per_row: self.words_per_row,
            rows: self.rows,
            cols: self.cols,
            rules: self.rules,
            generation: self.generation,
            kernel: K::default(),
        }
    }

    /// Re-tag this board to simulate under a different rule.
    fn with_rules(&self, rules: &Rules) -> Self {
        let mut b = self.clone();
        b.rules = *rules;
        b
    }

    fn pad(&self, top: isize, right: isize, bottom: isize, left: isize) -> Self {
        let board: Board = self.into();
        let padded = board.pad(top, right, bottom, left);
        let mut bits = Self::from(&padded);
        bits.rules = self.rules;
        bits.generation = self.generation;
        bits
    }

    fn iter(&self) -> impl Iterator<Item = bool> + '_ {
        let current = &self.current;
        let wp = self.words_per_row;
        // Each word contributes its low 64 bits, except the final word of
        // every row, which holds only `cols % 64` valid cells (a multiple of
        // 64 keeps a full final word).
        let last_nbits = {
            let v = self.cols % BITS;
            if v == 0 { BITS } else { v }
        };
        (0..current.len()).flat_map(move |idx| {
            let word = current[idx];
            let n = if idx % wp == wp - 1 { last_nbits } else { BITS };
            (0..n).map(move |i| (word >> i) & 1 == 1)
        })
    }
}

impl<K: Kernel> fmt::Display for BitBoard<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `iter` yields every cell in row-major order; a newline ends each
        // row, but not the last.
        for (i, live) in self.iter().enumerate() {
            f.write_char(if live { LIVE_CELL } else { DEAD_CELL })?;
            if (i + 1) % self.cols() == 0 && i + 1 < self.len() {
                f.write_char('\n')?;
            }
        }
        Ok(())
    }
}

impl<K: Kernel> FromStr for BitBoard<K> {
    type Err = crate::board::ParseBoardErr;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Board::from_str(s).map(|board| Self::from(&board))
    }
}

/// Re-pack a `BitBoard` into a `Vec<bool>` `Board` by rendering and re-parsing.
/// Used by the bit-packed `pad`, which re-packs words when the width changes.
impl<K: Kernel> From<&BitBoard<K>> for Board {
    fn from(bits: &BitBoard<K>) -> Board {
        Board::from_str(&bits.to_string()).expect("BitBoard renders to valid board text")
    }
}

/// Build a `BitBoard` from a `Vec<bool>` `Board`'s cell stream, so the two
/// representations can be compared generation-for-generation.
impl<K: Kernel> From<&Board> for BitBoard<K> {
    fn from(board: &Board) -> Self {
        let mut bits = Self::new(board.rows(), board.cols());
        for (i, live) in board.iter().enumerate() {
            let row = i / board.cols();
            let col = i % board.cols();
            if live {
                bits.current[row * bits.words_per_row + col / BITS] |= 1u64 << (col % BITS);
            }
        }
        bits
    }
}

impl<K: Kernel> PartialEq for BitBoard<K> {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation
            && self.rows == other.rows
            && self.cols == other.cols
            && self.current == other.current
            && self.rules == other.rules
    }
}

impl<K: Kernel> Eq for BitBoard<K> {}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_text(text: &str) -> (Board, ScalarBitBoard) {
        let board = Board::from_str(text).unwrap();
        (board.clone(), ScalarBitBoard::from(&board))
    }

    #[test]
    fn test_blinker_population() {
        let (mut board, mut bits) = from_text(".@.\n.@.\n.@.");
        for _ in 0..4 {
            board = board.next_generation();
            bits.step();
        }
        assert_eq!(bits.population(), 3);
        assert_eq!(board.population(), bits.population());
    }

    #[test]
    fn test_glider_matches_board() {
        let (board, bits) = from_text("...\n..@\n@..\n.@.");
        let board_next = board.next_generation();
        let mut bits_next = bits.clone();
        bits_next.step();
        assert_eq!(board_next.population(), bits_next.population());
        assert_eq!(board_next.to_string(), bits_next.to_string());
    }

    #[test]
    fn test_multi_generation_parity() {
        // Large enough to span multiple words per row (exercises cross-word
        // neighbor counting) and run several generations.
        let mut board = Board::new(200, 200).random();
        let mut bits = ScalarBitBoard::from(&board);
        for g in 0..8 {
            let b1 = board.to_string();
            let b2 = bits.to_string();
            assert_eq!(b1, b2, "mismatch at generation {g}");
            board = board.next_generation();
            bits.step();
        }
        assert_eq!(board.generation(), 8);
        assert_eq!(bits.generation(), 8);
        assert_eq!(LifeBoard::generation(&bits), 8);
    }

    #[test]
    fn test_parity_single_word_per_row() {
        // A board whose rows fit in one word per row (cols < 64).
        let mut board = Board::new(7, 40).random();
        let mut bits = ScalarBitBoard::from(&board);
        for _ in 0..5 {
            assert_eq!(board.to_string(), bits.to_string());
            board = board.next_generation();
            bits.step();
        }
    }

    #[test]
    fn test_parity_at_word_boundary() {
        // 64-wide board with a vertical blinker in column 63 (the last bit
        // of a word), so its other neighbors fall in the next word.
        let mut row = vec![DEAD_CELL; 64];
        row[63] = LIVE_CELL;
        let line: String = row.iter().collect();
        let text = format!("{line}\n{line}\n{line}");
        let (mut board, mut bits) = from_text(&text);
        for _ in 0..4 {
            board = board.next_generation();
            bits.step();
            assert_eq!(board.to_string(), bits.to_string());
        }
    }

    #[test]
    fn test_random_population_reasonable() {
        let bits = ScalarBitBoard::new(64, 64).random();
        // ~50% on average, allow a wide band for the random case.
        let pop = bits.population();
        assert!(pop > 64 * 64 / 2 - 100 && pop < 64 * 64 / 2 + 100);
    }

    #[test]
    fn test_lifeboard_methods_match_board() {
        use crate::lifeboard::LifeBoard;
        // 70 cols spans two words with a partial last word, so it exercises
        // the padding handling in the bit-packed paths.
        let board = Board::new(50, 70).random();
        let bits = ScalarBitBoard::from(&board);

        // toggle a cell; an out-of-bounds toggle is a no-op
        assert_eq!(
            board.toggle(3, 5).to_string(),
            bits.toggle(3, 5).to_string()
        );
        assert_eq!(bits.to_string(), bits.toggle(999, 999).to_string());

        // clear
        assert_eq!(board.clear().population(), bits.clear().population());
        assert_eq!(board.clear().to_string(), bits.clear().to_string());

        // random keeps the dimensions
        let r = bits.random();
        assert_eq!((r.rows(), r.cols()), (bits.rows(), bits.cols()));

        // pad re-packs the words (exercises the render/re-parse round-trip)
        let p_b = board.pad(2, 3, 1, 1);
        let p_t = bits.pad(2, 3, 1, 1);
        assert_eq!((p_b.rows(), p_b.cols()), (p_t.rows(), p_t.cols()));
        assert_eq!(p_b.to_string(), p_t.to_string());

        // iter() yields the same row-major stream on both backends
        let bcells: Vec<bool> = board.iter().collect();
        let tcells: Vec<bool> = bits.iter().collect();
        assert_eq!(bcells, tcells);
    }

    #[test]
    fn test_len() {
        let b = ScalarBitBoard::new(3, 4);
        assert_eq!(b.len(), 12);
    }

    #[test]
    fn test_next_generation() {
        // A glider: advancing via the (value-returning) path must match the
        // reference board, generation counter included.
        let (board, bits) = from_text(".@.\n..@\n@@.");
        let next = bits.next_generation();
        assert_eq!(next.generation(), 1);
        assert_eq!(board.next_generation().population(), next.population());
    }

    #[test]
    fn test_partial_eq() {
        let a = ScalarBitBoard::new(3, 3);
        assert_eq!(a, ScalarBitBoard::new(3, 3));
        assert_ne!(a, ScalarBitBoard::new(3, 4)); // different shape
        let mut c = a.clone();
        c.step();
        assert_ne!(a, c); // different generation / cells
    }

    #[test]
    fn test_iter_word_boundary() {
        // A width that is an exact multiple of 64 exercises the `cols % 64 == 0`
        // branch of `iter` (the final word of each row keeps all 64 bits).
        let bits = ScalarBitBoard::new(3, 64).random();
        let n = bits.iter().filter(|c| *c).count();
        assert_eq!(n, bits.population());
    }

    #[test]
    fn test_every_count_general_path() {
        // A rule that births and survives on every neighbor count exercises all
        // branches of the general per-count path, including the count-8 fold.
        let rule = Rules::from_str("B012345678/S012345678").unwrap();
        let mut board = Board::new(17, 17).with_rules(&rule).random();
        let mut bits = ScalarBitBoard::from(&board).with_rules(&rule);
        for _ in 0..3 {
            assert_eq!(board.to_string(), bits.to_string());
            board = board.next_generation();
            bits.step();
        }
    }

    #[test]
    fn test_custom_rule_matches_board() {
        // A non-Conway rule must give the same generations on both backends,
        // exercising BitBoard's general per-count path against the reference.
        for rule in [
            Rules::from_str("B3/S1").unwrap(),
            Rules::from_str("B368/S245").unwrap(), // Day & Night
            Rules::from_str("B368/S4578").unwrap(), // 34
            Rules::from_str("B48/S12").unwrap(),
        ] {
            let mut board = Board::new(30, 30).with_rules(&rule).random();
            let mut bits = ScalarBitBoard::from(&board).with_rules(&rule);
            for g in 0..6 {
                assert_eq!(
                    board.to_string(),
                    bits.to_string(),
                    "rule {rule:?} mismatch at generation {g}"
                );
                board = board.next_generation();
                bits.step();
            }
        }
    }

    #[test]
    fn test_from_str_error() {
        // A malformed template and a row-length mismatch must both propagate
        // the parse error through `FromStr` (covers the `?` on `from_str`).
        assert!(ScalarBitBoard::from_str("X").is_err());
        assert!(ScalarBitBoard::from_str("@@\n@").is_err());
    }

    #[test]
    fn test_display_fails_on_char_write() {
        // A writer whose `write_str` always errors makes `Display::fmt`'s
        // `?` propagate on the first character write (covers the char `?`).
        struct AlwaysFail;
        impl Write for AlwaysFail {
            fn write_str(&mut self, _: &str) -> fmt::Result {
                Err(fmt::Error)
            }
        }
        let mut w = AlwaysFail;
        let bits = ScalarBitBoard::new(3, 3);
        assert!(write!(w, "{bits}").is_err());
    }

    #[test]
    fn test_display_fails_on_newline_write() {
        // A writer that tolerates characters but rejects a newline makes the
        // newline `?` propagate. A multi-row board is needed so a newline is
        // emitted between rows.
        struct FailOnNewline;
        impl Write for FailOnNewline {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                if s.contains('\n') {
                    Err(fmt::Error)
                } else {
                    Ok(())
                }
            }
        }
        let mut w = FailOnNewline;
        let bits = ScalarBitBoard::new(3, 3);
        assert!(write!(w, "{bits}").is_err());
    }

    #[cfg(feature = "rayon")]
    #[test]
    fn test_parallel_scalar_matches_serial() {
        // The row-parallel kernel must produce identical generations to the
        // serial kernel and the reference board, across a wide and a narrow
        // (odd words/row) board.
        let rule = Rules::from_str("B368/S245").unwrap(); // Day & Night
        for cols in [40usize, 130] {
            let mut board = Board::new(50, cols).with_rules(&rule).random();
            let mut serial = ScalarBitBoard::from(&board).with_rules(&rule);
            let mut parallel = ParallelScalarBitBoard::from(&board).with_rules(&rule);
            for g in 0..4 {
                assert_eq!(serial.to_string(), board.to_string());
                assert_eq!(
                    parallel.to_string(),
                    board.to_string(),
                    "parallel scalar mismatch at gen {g}, {cols} cols"
                );
                board = board.next_generation();
                serial.step();
                parallel.step();
            }
        }
    }

    #[cfg(all(feature = "rayon", feature = "unstable"))]
    #[test]
    fn test_parallel_simd_matches_serial() {
        // The row-parallel SIMD kernel must agree with the serial SIMD kernel
        // and the reference board under a non-Conway rule.
        let rule = Rules::from_str("B368/S245").unwrap(); // Day & Night
        let mut board = Board::new(130, 130).with_rules(&rule).random();
        let mut serial = SimdBitBoard::from(&board).with_rules(&rule);
        let mut parallel = ParallelSimdBitBoard::from(&board).with_rules(&rule);
        for g in 0..4 {
            assert_eq!(serial.to_string(), board.to_string());
            assert_eq!(
                parallel.to_string(),
                board.to_string(),
                "parallel simd mismatch at gen {g}"
            );
            board = board.next_generation();
            serial.step();
            parallel.step();
        }
    }
}
