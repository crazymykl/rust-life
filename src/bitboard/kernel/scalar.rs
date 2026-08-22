//! The default, word-by-word bit-parallel kernel for `BitBoard`: it walks the
//! board one word at a time through the shared `threshold` and `zero_padding`
//! helpers in the parent module.

use super::{Kernel, StepCtx, threshold, zero_padding};

/// The default, word-by-word bit-parallel kernel.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ScalarKernel;

impl Kernel for ScalarKernel {
    fn compute(&self, current: &[u64], next: &mut [u64], ctx: &StepCtx) {
        let wp = ctx.words_per_row;
        for r in 0..ctx.rows {
            let top = if r == 0 {
                &[]
            } else {
                &current[(r - 1) * wp..r * wp]
            };
            let mid = &current[r * wp..(r + 1) * wp];
            let bot = if r + 1 >= ctx.rows {
                &[]
            } else {
                &current[(r + 1) * wp..(r + 2) * wp]
            };
            for wc in 0..wp {
                next[r * wp + wc] = threshold(top, mid, bot, wp, wc, ctx.rules);
            }
        }
        zero_padding(next, ctx.cols, ctx.rows, wp);
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
            row.iter_mut().enumerate().for_each(|(wc, word)| {
                *word = threshold(top, mid, bot, wp, wc, rules);
            });
        });
        zero_padding(next, ctx.cols, rows, wp);
    }
}
