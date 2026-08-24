//! The `std::simd`-accelerated board: a `BitBoard` whose `step` runs the pair
//! kernel over two adjacent 64-cell words at once with `u64x2`. It exposes the
//! same `LifeBoard` surface, delegating everything to the wrapped board except
//! `step`, which uses `BitBoard::step_with` to supply the SIMD kernel over the
//! double-buffered words. This module — and its `--backend simd` selection — is
//! available only under the `unstable` feature.

use core::simd::prelude::*;

use crate::Rules;
use crate::bitboard::BitBoard;
use crate::board::Board;
use crate::life::LifeBoard;
use std::fmt;
use std::str::FromStr;

/// A `std::simd`-accelerated board: a `BitBoard` whose `step` runs the pair
/// kernel over two adjacent words at once. It exposes the same `LifeBoard`
/// surface, delegating everything to the wrapped board except `step`.
#[derive(Clone, Debug)]
pub struct SimdBoard(BitBoard);

impl fmt::Display for SimdBoard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for SimdBoard {
    type Err = crate::board::ParseBoardErr;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SimdBoard(BitBoard::from_str(s)?))
    }
}

impl From<&Board> for SimdBoard {
    fn from(board: &Board) -> Self {
        SimdBoard(BitBoard::from(board))
    }
}

impl PartialEq for SimdBoard {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for SimdBoard {}

impl LifeBoard for SimdBoard {
    fn new(rows: usize, cols: usize) -> Self {
        SimdBoard(BitBoard::new(rows, cols))
    }

    fn rows(&self) -> usize {
        self.0.rows()
    }

    fn cols(&self) -> usize {
        self.0.cols()
    }

    fn generation(&self) -> usize {
        self.0.generation()
    }

    fn population(&self) -> usize {
        self.0.population()
    }

    fn next_generation(&self) -> Self {
        SimdBoard(self.0.next_generation())
    }

    /// The two-words-at-once SIMD path: the pair kernel runs over the wrapped
    /// board's double buffer, supplied through `BitBoard::step_with`.
    fn step(&mut self) {
        self.0.step_with(|current, next, board| {
            let is_conway = board.rules.is_conway();
            let born = board.rules.born_mask();
            let survive = board.rules.survive_mask();
            let rows = board.rows();
            let wp = board.words_per_row;

            for r in 0..rows {
                let top = if r == 0 {
                    &[]
                } else {
                    &current[(r - 1) * wp..r * wp]
                };
                let mid = &current[r * wp..(r + 1) * wp];
                let bot = if r + 1 >= rows {
                    &[]
                } else {
                    &current[(r + 1) * wp..(r + 2) * wp]
                };

                let mut wc = 0;
                while wc + 1 < wp {
                    next[r * wp + wc..r * wp + wc + 2].copy_from_slice(&threshold_pair(
                        top, mid, bot, wc, is_conway, born, survive,
                    ));
                    wc += 2;
                }
                if wc < wp {
                    next[r * wp + wc] = board.threshold(current, wp, r, wc);
                }
            }
            board.zero_padding(next, rows, wp);
        });
    }

    fn with_rules(&self, rules: &Rules) -> Self {
        SimdBoard(self.0.with_rules(rules))
    }

    fn toggle(&self, x: usize, y: usize) -> Self {
        SimdBoard(self.0.toggle(x, y))
    }

    fn clear(&self) -> Self {
        SimdBoard(self.0.clear())
    }

    fn random(&self) -> Self {
        SimdBoard(self.0.random())
    }

    fn pad(&self, top: isize, right: isize, bottom: isize, left: isize) -> Self {
        SimdBoard(self.0.pad(top, right, bottom, left))
    }

    fn iter(&self) -> impl Iterator<Item = bool> + '_ {
        self.0.iter()
    }
}

// Bit-parallel next words for the pair [wc, wc+1], computed with `u64x2`. Same
// bit-parallel formula as the scalar `BitBoard::threshold`, but the neighbor
// bitboards and the odd/even reduction run on two words at once.
#[inline]
fn threshold_pair(
    top: &[u64],
    mid: &[u64],
    bot: &[u64],
    wc: usize,
    is_conway: bool,
    born: u32,
    survive: u32,
) -> [u64; 2] {
    let (tl, tc, tr) = pair3(top, wc);
    let (ml, mc, mr) = pair3(mid, wc);
    let (bl, bc, br) = pair3(bot, wc);

    let n_tl = tc << 1 | tl >> 63;
    let n_tr = tc >> 1 | tr << 63;
    let n_ml = mc << 1 | ml >> 63;
    let n_mr = mc >> 1 | mr << 63;
    let n_bl = bc << 1 | bl >> 63;
    let n_br = bc >> 1 | br << 63;

    let t_odd = n_tl ^ tc ^ n_tr;
    let t_even = (n_tl & tc) | (tc & n_tr) | (n_tl & n_tr);
    let m_odd = n_ml ^ n_mr;
    let m_even = n_ml & n_mr;
    let b_odd = n_bl ^ bc ^ n_br;
    let b_even = (n_bl & bc) | (bc & n_br) | (n_bl & n_br);

    let bit0 = t_odd ^ m_odd ^ b_odd;
    let c01 = (t_odd & m_odd) | (t_odd & b_odd) | (m_odd & b_odd);
    let bit1 = (t_even ^ m_even ^ b_even) ^ c01;
    let c02 = (t_even & m_even)
        | (t_even & b_even)
        | (m_even & b_even)
        | (t_even & c01)
        | (m_even & c01)
        | (b_even & c01);

    // Fast path: Conway's B3/S23 is one branchless expression.
    if is_conway {
        let next = bit1 & !c02 & (bit0 | mc);
        return [next[0], next[1]];
    }

    // General path: mirror the scalar `threshold` per live-neighbor count.
    // `born`/`survive` are scalar, so the branch conditions are the same
    // for both SIMD lanes; a count of 8 folds onto count 4's 3-bit pattern.
    let all8 = n_tl & tc & n_tr & n_ml & n_mr & n_bl & bc & n_br;
    let mut next = u64x2::splat(0);
    for n in 0..8 {
        let mut m = BitBoard::select(bit0, n & 1 != 0)
            & BitBoard::select(bit1, n & 2 != 0)
            & BitBoard::select(c02, n & 4 != 0);
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
    [next[0], next[1]]
}

// (left, center, right) neighbor word-vectors for the pair [wc, wc+1], each a
// `u64x2` over the two lanes. Out-of-range lanes are zero, so the empty
// top/bottom rows and the left/right edges behave as dead cells.
#[inline]
fn pair3(row: &[u64], wc: usize) -> (u64x2, u64x2, u64x2) {
    let a = w(row, wc as isize - 1);
    let b = w(row, wc as isize);
    let c = w(row, wc as isize + 1);
    let d = w(row, wc as isize + 2);
    (
        u64x2::from_array([a, b]),
        u64x2::from_array([b, c]),
        u64x2::from_array([c, d]),
    )
}

#[inline]
fn w(row: &[u64], i: isize) -> u64 {
    if i < 0 {
        0
    } else {
        row.get(i as usize).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::bitboard_to_str;

    // The SIMD path must agree with both the scalar bit-parallel path and
    // the reference `Board`, generation-for-generation.
    #[test]
    fn test_simd_matches_scalar() {
        let mut board = Board::new(130, 130).random();
        let mut scalar = BitBoard::from(&board);
        let mut simd = SimdBoard::from(&board);
        for g in 0..6 {
            board = board.next_generation();
            scalar.step();
            simd.step();
            assert_eq!(
                bitboard_to_str(&scalar),
                format!("{simd}"),
                "scalar/simd mismatch at gen {g}"
            );
            assert_eq!(
                board.to_string(),
                format!("{simd}"),
                "board/simd mismatch at gen {g}"
            );
        }
    }

    // The SIMD backend must agree with the reference `Board` under a non-Conway
    // rule, exercising the generalized `threshold_pair` general path. 130 cols
    // → 3 words/row, so both the pair loop and the scalar tail run.
    #[test]
    fn test_simd_backend_non_conway() {
        let rule = Rules::from_str("B368/S245").unwrap(); // Day & Night
        let mut board = Board::new(130, 130).with_rules(&rule).random();
        let mut simd = SimdBoard::from(&board).with_rules(&rule);
        for g in 0..6 {
            assert_eq!(
                board.to_string(),
                format!("{simd}"),
                "Day & Night simd mismatch at generation {g}"
            );
            board = board.next_generation();
            simd.step();
        }
    }

    // A rule that births and survives on every neighbor count exercises
    // all branches of the SIMD general path, including the count-8 fold
    // in both the born and survive directions.
    #[test]
    fn test_simd_every_count() {
        let rule = Rules::from_str("B012345678/S012345678").unwrap();
        let mut board = Board::new(130, 130).with_rules(&rule).random();
        let mut simd = SimdBoard::from(&board).with_rules(&rule);
        for g in 0..3 {
            assert_eq!(
                board.to_string(),
                format!("{simd}"),
                "simd every-count mismatch at generation {g}"
            );
            board = board.next_generation();
            simd.step();
        }
    }

    // A non-Conway rule with count 8 in neither the birth nor survival set
    // exercises the `born & (1<<8)` / `survive & (1<<8)` false branches, which
    // `test_simd_every_count` (which includes 8) leaves uncovered.
    #[test]
    fn test_simd_no_count_8() {
        let rule = Rules::from_str("B3/S24").unwrap(); // HighLife
        let mut board = Board::new(130, 130).with_rules(&rule).random();
        let mut simd = SimdBoard::from(&board).with_rules(&rule);
        for g in 0..3 {
            assert_eq!(
                board.to_string(),
                format!("{simd}"),
                "simd no-count-8 mismatch at generation {g}"
            );
            board = board.next_generation();
            simd.step();
        }
    }

    // Exercises `SimdBoard`'s `FromStr` (success and error) and its
    // `PartialEq`/`Display`, the glue not reached through `exercise`.
    #[test]
    fn test_simd_board_parse_and_eq() {
        let a = SimdBoard::from_str("@\n.\n.").unwrap();
        let b = SimdBoard::from_str("@\n.\n.").unwrap();
        let c = SimdBoard::from_str(".\n.\n.").unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a.to_string(), c.to_string());
        assert!(SimdBoard::from_str("X").is_err());
    }
}
