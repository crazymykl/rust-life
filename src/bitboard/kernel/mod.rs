//! The kernel abstraction for the bit-packed board.
//!
//! A [`Kernel`] turns the current word buffer into the next generation and is a
//! stateless tag selected at the type level: the [`ScalarKernel`] in `scalar.rs`
//! walks the board one word at a time, while the `std::simd` [`SimdKernel`] in
//! `simd.rs` (gated behind the `unstable` feature) does two adjacent words at
//! once. Each also has a rayon row-parallel variant.
//!
//! The kernels share the bit-parallel helpers below: [`threshold`] computes one
//! next-generation word, and [`zero_padding`] clears the padding bits of each
//! row's final word.

mod scalar;

#[cfg(feature = "unstable")]
mod simd;

#[cfg(feature = "rayon")]
pub(crate) use scalar::ParallelScalarKernel;
pub(crate) use scalar::ScalarKernel;
#[cfg(all(feature = "rayon", feature = "unstable"))]
pub(crate) use simd::ParallelSimdKernel;
#[cfg(feature = "unstable")]
pub(crate) use simd::SimdKernel;

use super::BITS;
use crate::Rules;
use std::fmt::Debug;

/// How a `BitBoard`'s double-buffered `step` turns the current word buffer into
/// the next one. `ScalarKernel` walks the board one word at a time; the
/// `std::simd` `SimdKernel` (in `simd.rs`, `unstable` only) does two
/// adjacent words at once. A kernel is a stateless tag, so it derives `Default`.
pub(crate) trait Kernel: Clone + Debug + Default {
    /// Fill `next` with the next generation of `current`, using `ctx` for the
    /// board's rule and dimensions.
    fn compute(&self, current: &[u64], next: &mut [u64], ctx: &StepCtx);
}

/// The per-step facts a kernel needs: the rule and the dimensions governing
/// neighbor slicing and padding.
pub(crate) struct StepCtx<'a> {
    pub rules: &'a Rules,
    pub rows: usize,
    pub words_per_row: usize,
    pub cols: usize,
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
fn select<P: Copy + std::ops::Not<Output = P>>(plane: P, set: bool) -> P {
    if set { plane } else { !plane }
}

// Compute the next-generation word for one 64-cell word at (r, wc) with a
// fully bit-parallel formula (no per-cell loop). The 3x3 neighborhood is
// built as eight neighbor bitboards and reduced to 3 count bits; Conway's
// B3/S23 is a single branchless expression, while any other rule is
// applied per live-neighbor count (see the general path at the end).
#[inline]
fn threshold(
    top: &[u64],
    mid: &[u64],
    bot: &[u64],
    per_row: usize,
    wc: usize,
    rules: &Rules,
) -> u64 {
    // Eight neighbor bitboards for the 64 cells of this word.
    let (tl, tc, tr) = row3(top, per_row, wc);
    let (ml, mc, mr) = row3(mid, per_row, wc);
    let (bl, bc, br) = row3(bot, per_row, wc);

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
    if rules.is_conway() {
        return bit1 & !c02 & (bit0 | mc);
    }

    // General path: apply the rule per live-neighbor count. The three count
    // bits (bit0, bit1, c02) distinguish counts 0..7; a count of 8 folds onto
    // the count-4 pattern, so its cells are pulled out separately as `all8`.
    let all8 = n_tl & tc & n_tr & n_ml & n_mr & n_bl & bc & n_br;
    let born = rules.born_mask();
    let survive = rules.survive_mask();
    let mut next = 0u64;
    for n in 0..8 {
        let mut m = select(bit0, n & 1 != 0) & select(bit1, n & 2 != 0) & select(c02, n & 4 != 0);
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

// Zero the padding bits of the last word of each row, so that cells
// beyond `cols` never become phantom live neighbors of real cells.
#[inline]
fn zero_padding(dst: &mut [u64], cols: usize, rows: usize, wp: usize) {
    let valid = cols % BITS;
    let mask = if valid == 0 {
        !0u64
    } else {
        (1u64 << valid) - 1
    };
    for r in 0..rows {
        dst[r * wp + wp - 1] &= mask;
    }
}
