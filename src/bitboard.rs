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
//! A `std::simd` variant (`SimdBoard::step`, in `simd_board.rs`) runs the same
//! formula on two adjacent words at once with `u64x2`. It is gated behind the
//! `unstable` feature.

use crate::Rules;
use crate::board::Board;
use crate::life::LifeBoard;
use std::fmt::{self, Write};
use std::str::FromStr;

const BITS: usize = 64;

#[derive(Clone, Debug)]
pub struct BitBoard {
    // Double buffer: the current generation's cells.
    current: Vec<u64>,
    // Scratch buffer for the next generation; swapped with `current` on step.
    next: Vec<u64>,
    pub(crate) words_per_row: usize,
    rows: usize,
    cols: usize,
    // The rule to apply. When it's Conway's B3/S23 the branchless fast path
    // runs; otherwise the general per-neighbor-count path does.
    pub(crate) rules: Rules,
    generation: usize,
}

impl BitBoard {
    pub fn new(rows: usize, cols: usize) -> BitBoard {
        Self::new_with_rules(rows, cols, &Rules::conway())
    }

    /// A fresh, all-dead board of the given size that simulates under `rules`.
    pub fn new_with_rules(rows: usize, cols: usize, rules: &Rules) -> BitBoard {
        let words_per_row = cols.div_ceil(BITS);
        let words = vec![0u64; rows * words_per_row];
        BitBoard {
            current: words.clone(),
            next: words,
            words_per_row,
            rows,
            cols,
            rules: *rules,
            generation: 0,
        }
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn generation(&self) -> usize {
        self.generation
    }

    pub fn population(&self) -> usize {
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

    // Fetch the (left, center, right) source words for a single row at word
    // column `wc`, treating out-of-bounds neighbors as dead (0). `row` is a
    // slice already positioned at the start of that generation row.
    #[inline]
    fn row3(row: &[u64], per_row: usize, wc: usize) -> (u64, u64, u64) {
        if row.is_empty() {
            return (0, 0, 0);
        }
        let c = row[wc];
        let l = if wc == 0 { 0 } else { row[wc - 1] };
        let r = if wc + 1 < per_row { row[wc + 1] } else { 0 };
        (l, c, r)
    }

    // Pick `plane` or its complement, so a count bit can be matched against a
    // target bit (1) or its absence (0) without a branch.
    #[inline]
    pub(crate) fn select<P: Copy + std::ops::Not<Output = P>>(plane: P, set: bool) -> P {
        if set { plane } else { !plane }
    }

    // Compute the next-generation word for one 64-cell word at (r, wc) with a
    // fully bit-parallel formula (no per-cell loop). The 3x3 neighborhood is
    // built as eight neighbor bitboards and reduced to 3 count bits; Conway's
    // B3/S23 is a single branchless expression, while any other rule is
    // applied per live-neighbor count (see the general path at the end).
    #[inline]
    pub(crate) fn threshold(&self, src: &[u64], per_row: usize, r: usize, wc: usize) -> u64 {
        let top = if r == 0 {
            &[]
        } else {
            &src[(r - 1) * per_row..r * per_row]
        };
        let mid = &src[r * per_row..(r + 1) * per_row];
        let bot = if r + 1 >= self.rows {
            &[]
        } else {
            &src[(r + 1) * per_row..(r + 2) * per_row]
        };

        // Eight neighbor bitboards for the 64 cells of this word.
        let (tl, tc, tr) = Self::row3(top, per_row, wc);
        let (ml, mc, mr) = Self::row3(mid, per_row, wc);
        let (bl, bc, br) = Self::row3(bot, per_row, wc);

        let n_tl = tc << 1 | tl >> 63;
        let n_tr = tc >> 1 | tr << 63;
        let n_ml = mc << 1 | ml >> 63;
        let n_mr = mc >> 1 | mr << 63;
        let n_bl = bc << 1 | bl >> 63;
        let n_br = bc >> 1 | br << 63;

        // Per-row (odd, even) count of live neighbors: odd = an odd number of
        // the row's cells are live, even = two or more are live, so a row's
        // contribution to the total is 2*even + odd. The middle row omits the
        // center cell, so it uses only left & right.
        let t_odd = n_tl ^ tc ^ n_tr;
        let t_even = (n_tl & tc) | (tc & n_tr) | (n_tl & n_tr);
        let m_odd = n_ml ^ n_mr;
        let m_even = n_ml & n_mr;
        let b_odd = n_bl ^ bc ^ n_br;
        let b_even = (n_bl & bc) | (bc & n_br) | (n_bl & n_br);

        // Combine the three rows into bit0/bit1/bit2 of the 3x3 count.
        let bit0 = t_odd ^ m_odd ^ b_odd;
        let c01 = (t_odd & m_odd) | (t_odd & b_odd) | (m_odd & b_odd);
        let bit1 = (t_even ^ m_even ^ b_even) ^ c01;
        let c02 = (t_even & m_even)
            | (t_even & b_even)
            | (m_even & b_even)
            | (t_even & c01)
            | (m_even & c01)
            | (b_even & c01);

        // Fast path: Conway's B3/S23 is exactly `bit1 & !c02 & (bit0 | center)`.
        if self.rules.is_conway() {
            return bit1 & !c02 & (bit0 | mc);
        }

        // General path: apply the rule per live-neighbor count. The three count
        // bits (bit0, bit1, c02) distinguish counts 0..7; a count of 8 folds onto
        // the count-4 pattern, so its cells are pulled out separately as `all8`.
        let all8 = n_tl & tc & n_tr & n_ml & n_mr & n_bl & bc & n_br;
        let born = self.rules.born_mask();
        let survive = self.rules.survive_mask();
        let mut next = 0u64;
        for n in 0..8 {
            let mut m = Self::select(bit0, n & 1 != 0)
                & Self::select(bit1, n & 2 != 0)
                & Self::select(c02, n & 4 != 0);
            if n == 4 {
                m &= !all8; // count 8 shares count 4's 3-bit pattern
            }
            if born & (1 << n) != 0 {
                next |= m & !mc;
            }
            if survive & (1 << n) != 0 {
                next |= m & mc;
            }
        }
        // Count 8: all eight neighbors are live.
        if born & (1 << 8) != 0 {
            next |= all8 & !mc;
        }
        if survive & (1 << 8) != 0 {
            next |= all8 & mc;
        }
        next
    }

    // Fill `dst` with the next generation of `src`, one word at a time.
    #[inline]
    fn count_neighbors(&self, src: &[u64], dst: &mut [u64]) {
        let rows = self.rows;
        let wp = self.words_per_row;
        for r in 0..rows {
            for wc in 0..wp {
                dst[r * wp + wc] = self.threshold(src, wp, r, wc);
            }
        }
        self.zero_padding(dst, rows, wp);
    }

    // Zero the padding bits of the last word of each row, so that cells
    // beyond `cols` never become phantom live neighbors of real cells.
    #[inline]
    pub(crate) fn zero_padding(&self, dst: &mut [u64], rows: usize, wp: usize) {
        let valid = self.cols % BITS;
        let mask = if valid == 0 {
            !0u64
        } else {
            (1u64 << valid) - 1
        };
        for r in 0..rows {
            dst[r * wp + wp - 1] &= mask;
        }
    }

    /// Advance one generation **in place**, reusing the two word buffers
    /// (no allocation). This is the double-buffered, word-level, bit-parallel
    /// path.
    pub fn step(&mut self) {
        let current = std::mem::take(&mut self.current);
        let mut next = std::mem::take(&mut self.next);
        self.count_neighbors(&current, &mut next);
        self.current = next;
        self.next = current;
        self.generation += 1;
    }

    /// Double-buffered in-place advance: compute the next buffer from the
    /// current one via `compute`, then swap and bump the generation. The SIMD
    /// backend (`SimdBoard`) uses this to supply its own kernel.
    #[cfg(feature = "unstable")]
    pub(crate) fn step_with<F>(&mut self, compute: F)
    where
        F: FnOnce(&[u64], &mut [u64], &Self),
    {
        let current = std::mem::take(&mut self.current);
        let mut next = std::mem::take(&mut self.next);
        compute(&current, &mut next, self);
        self.current = next;
        self.next = current;
        self.generation += 1;
    }

    /// Compute a brand-new generation (allocates). Kept for the cross-check
    /// tests that expect `let next = bits.next_generation();`.
    #[allow(dead_code)]
    pub fn next_generation(&self) -> BitBoard {
        let mut b = self.clone();
        b.step();
        b
    }

    pub fn random(&self) -> BitBoard {
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
        }
    }
}

impl fmt::Display for BitBoard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let wp = self.words_per_row;
        for r in 0..self.rows {
            for c in 0..self.cols {
                let idx = r * wp + c / BITS;
                let live = (self.current[idx] >> (c % BITS)) & 1 == 1;
                f.write_char(if live { '@' } else { '.' })?;
            }
            if r + 1 < self.rows {
                f.write_char('\n')?;
            }
        }
        Ok(())
    }
}

impl FromStr for BitBoard {
    type Err = crate::board::ParseBoardErr;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let board = Board::from_str(s)?;
        Ok(Self::from(&board))
    }
}

/// Re-pack a `BitBoard` into a `Vec<bool>` `Board` by rendering and re-parsing.
/// Used by the bit-packed `pad`, which re-packs words when the width changes.
impl From<&BitBoard> for Board {
    fn from(bits: &BitBoard) -> Board {
        Board::from_str(&format!("{bits}")).expect("BitBoard renders to valid board text")
    }
}

impl PartialEq for BitBoard {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation
            && self.rows == other.rows
            && self.cols == other.cols
            && self.current == other.current
    }
}

impl Eq for BitBoard {}

impl LifeBoard for BitBoard {
    fn new(rows: usize, cols: usize) -> Self {
        BitBoard::new(rows, cols)
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

    fn population(&self) -> usize {
        self.population()
    }

    fn next_generation(&self) -> Self {
        BitBoard::next_generation(self)
    }

    /// The double-buffered, allocation-free fast path.
    fn step(&mut self) {
        BitBoard::step(self);
    }

    fn toggle(&self, x: usize, y: usize) -> Self {
        let mut b = self.clone();
        if x < self.rows && y < self.cols {
            b.current[x * self.words_per_row + y / BITS] ^= 1u64 << (y % BITS);
        }
        b
    }

    fn clear(&self) -> Self {
        BitBoard::new_with_rules(self.rows, self.cols, &self.rules)
    }

    fn random(&self) -> Self {
        BitBoard::random(self)
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
        let mut bits = BitBoard::from(&padded);
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

/// Build a `BitBoard` from a `Vec<bool>` `Board`'s cell stream, so the two
/// representations can be compared generation-for-generation.
impl From<&Board> for BitBoard {
    fn from(board: &Board) -> BitBoard {
        let mut bits = BitBoard::new(board.rows(), board.cols());
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

/// Render a `BitBoard` in the same text form as `Board`'s `Display` so the two
/// can be compared string-for-string.
#[cfg(test)]
pub fn bitboard_to_str(bits: &BitBoard) -> String {
    format!("{bits}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_text(text: &str) -> (Board, BitBoard) {
        let board = Board::from_str(text).unwrap();
        (board.clone(), BitBoard::from(&board))
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
        assert_eq!(board_next.to_string(), bitboard_to_str(&bits_next));
    }

    #[test]
    fn test_multi_generation_parity() {
        // Large enough to span multiple words per row (exercises cross-word
        // neighbor counting) and run several generations.
        let mut board = Board::new(200, 200).random();
        let mut bits = BitBoard::from(&board);
        for g in 0..8 {
            let b1 = board.to_string();
            let b2 = bitboard_to_str(&bits);
            assert_eq!(b1, b2, "mismatch at generation {g}");
            board = board.next_generation();
            bits.step();
        }
        assert_eq!(board.generation(), 8);
        assert_eq!(bits.generation(), 8);
    }

    #[test]
    fn test_parity_single_word_per_row() {
        // A board whose rows fit in one word per row (cols < 64).
        let mut board = Board::new(7, 40).random();
        let mut bits = BitBoard::from(&board);
        for _ in 0..5 {
            assert_eq!(board.to_string(), bitboard_to_str(&bits));
            board = board.next_generation();
            bits.step();
        }
    }

    #[test]
    fn test_parity_at_word_boundary() {
        // 64-wide board with a vertical blinker in column 63 (the last bit
        // of a word), so its other neighbors fall in the next word.
        let mut row = vec!['.'; 64];
        row[63] = '@';
        let line: String = row.iter().collect();
        let text = format!("{line}\n{line}\n{line}");
        let (mut board, mut bits) = from_text(&text);
        for _ in 0..4 {
            board = board.next_generation();
            bits.step();
            assert_eq!(board.to_string(), bitboard_to_str(&bits));
        }
    }

    #[test]
    fn test_random_population_reasonable() {
        let bits = BitBoard::new(64, 64).random();
        // ~50% on average, allow a wide band for the random case.
        let pop = bits.population();
        assert!(pop > 64 * 64 / 2 - 100 && pop < 64 * 64 / 2 + 100);
    }

    #[test]
    fn test_lifeboard_methods_match_board() {
        use crate::life::LifeBoard;
        // 70 cols spans two words with a partial last word, so it exercises
        // the padding handling in the bit-packed paths.
        let board = Board::new(50, 70).random();
        let bits = BitBoard::from(&board);

        // toggle a cell; an out-of-bounds toggle is a no-op
        assert_eq!(
            board.toggle(3, 5).to_string(),
            bitboard_to_str(&bits.toggle(3, 5))
        );
        assert_eq!(
            bitboard_to_str(&bits),
            bitboard_to_str(&bits.toggle(999, 999))
        );

        // clear
        assert_eq!(board.clear().population(), bits.clear().population());
        assert_eq!(board.clear().to_string(), bitboard_to_str(&bits.clear()));

        // random keeps the dimensions
        let r = bits.random();
        assert_eq!((r.rows(), r.cols()), (bits.rows(), bits.cols()));

        // pad re-packs the words (exercises the render/re-parse round-trip)
        let p_b = board.pad(2, 3, 1, 1);
        let p_t = bits.pad(2, 3, 1, 1);
        assert_eq!((p_b.rows(), p_b.cols()), (p_t.rows(), p_t.cols()));
        assert_eq!(p_b.to_string(), bitboard_to_str(&p_t));

        // iter() yields the same row-major stream on both backends
        let bcells: Vec<bool> = board.iter().collect();
        let tcells: Vec<bool> = bits.iter().collect();
        assert_eq!(bcells, tcells);
    }

    #[test]
    fn test_len() {
        let b = BitBoard::new(3, 4);
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
        let a = BitBoard::new(3, 3);
        assert_eq!(a, BitBoard::new(3, 3));
        assert_ne!(a, BitBoard::new(3, 4)); // different shape
        let mut c = a.clone();
        c.step();
        assert_ne!(a, c); // different generation / cells
    }

    #[test]
    fn test_iter_word_boundary() {
        // A width that is an exact multiple of 64 exercises the `cols % 64 == 0`
        // branch of `iter` (the final word of each row keeps all 64 bits).
        let bits = BitBoard::new(3, 64).random();
        let n = bits.iter().filter(|c| *c).count();
        assert_eq!(n, bits.population());
    }

    #[test]
    fn test_every_count_general_path() {
        // A rule that births and survives on every neighbor count exercises all
        // branches of the general per-count path, including the count-8 fold.
        let rule = Rules::from_str("B012345678/S012345678").unwrap();
        let mut board = Board::new(17, 17).with_rules(&rule).random();
        let mut bits = BitBoard::from(&board).with_rules(&rule);
        for _ in 0..3 {
            assert_eq!(board.to_string(), crate::bitboard::bitboard_to_str(&bits));
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
            let mut bits = BitBoard::from(&board).with_rules(&rule);
            for g in 0..6 {
                assert_eq!(
                    board.to_string(),
                    bitboard_to_str(&bits),
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
        assert!(BitBoard::from_str("X").is_err());
        assert!(BitBoard::from_str("@@\n@").is_err());
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
        let bits = BitBoard::new(3, 3);
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
        let bits = BitBoard::new(3, 3);
        assert!(write!(w, "{bits}").is_err());
    }
}
