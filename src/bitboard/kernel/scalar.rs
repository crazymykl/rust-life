//! The default, word-by-word bit-parallel kernel for `BitBoard`: it walks the
//! board one word at a time through the shared `threshold` and `zero_padding`
//! helpers in the parent module.

use crate::Rules;

use super::{Kernel, StepCtx, row_window, threshold, zero_padding};

/// The default, word-by-word bit-parallel kernel.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ScalarKernel;

impl Kernel for ScalarKernel {
    fn compute(&self, current: &[u64], next: &mut [u64], ctx: &StepCtx) {
        let rows = ctx.rows;
        let wp = ctx.words_per_row;
        let rules = ctx.rules;
        for r in 0..rows {
            let (top, mid, bot) = row_window(current, r, wp, rows);
            fill_row(&mut next[r * wp..(r + 1) * wp], top, mid, bot, rules);
        }
        zero_padding(next, ctx.cols, rows, wp);
    }
}

/// The rayon-parallel variant of the word-by-word kernel. Each output row
/// depends only on three adjacent `current` rows and writes a disjoint slice of
/// `next`, so the row loop runs conflict-free across the thread pool.
#[cfg(feature = "rayon")]
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ParallelScalarKernel;

#[cfg(feature = "rayon")]
impl Kernel for ParallelScalarKernel {
    fn compute(&self, current: &[u64], next: &mut [u64], ctx: &StepCtx) {
        let rows = ctx.rows;
        let wp = ctx.words_per_row;
        let rules = ctx.rules;
        use rayon::prelude::*;
        next.par_chunks_mut(wp).enumerate().for_each(|(r, row)| {
            let (top, mid, bot) = row_window(current, r, wp, rows);
            fill_row(row, top, mid, bot, rules);
        });
        zero_padding(next, ctx.cols, rows, wp);
    }
}

// Fill one output row (a `wp`-word slice) from its (top, mid, bot) window, one
// word at a time.
#[inline]
fn fill_row(row: &mut [u64], top: &[u64], mid: &[u64], bot: &[u64], rules: &Rules) {
    row.iter_mut().enumerate().for_each(|(wc, word)| {
        *word = threshold(top, mid, bot, wc, rules);
    });
}
