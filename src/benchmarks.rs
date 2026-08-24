extern crate test;

use self::test::Bencher;
use crate::Rules;
use crate::bitboard::BitBoard;
use crate::board::Board;
#[cfg(feature = "unstable")]
use crate::life::LifeBoard;
#[cfg(feature = "unstable")]
use crate::simd_board::SimdBoard;
use std::str::FromStr;

#[bench]
fn bench_random(b: &mut Bencher) {
    let brd = Board::new(200, 200);
    b.iter(|| brd.random());
}

#[bench]
fn bench_ten_generations(b: &mut Bencher) {
    let mut brd = Board::new(200, 200).random();
    b.iter(|| {
        for _ in 0..10 {
            brd = brd.serial_next_generation();
        }
    });
}

#[bench]
fn bench_ten_parallel_generations(b: &mut Bencher) {
    let mut brd = Board::new(200, 200).random();

    b.iter(|| {
        for _ in 0..10 {
            brd = brd.parallel_next_generation();
        }
    });
}

// ---- Bit-packed prototype benchmarks ----

#[bench]
fn bench_bitboard_ten_generations(b: &mut Bencher) {
    let mut brd = BitBoard::new(200, 200).random();

    b.iter(|| {
        for _ in 0..10 {
            brd = brd.next_generation();
        }
    });
}

#[bench]
fn bench_bitboard_large_ten_generations(b: &mut Bencher) {
    let mut brd = BitBoard::new(1000, 1000).random();

    b.iter(|| {
        for _ in 0..10 {
            brd = brd.next_generation();
        }
    });
}

#[bench]
fn bench_vecbool_large_ten_generations(b: &mut Bencher) {
    let mut brd = Board::new(1000, 1000).random();

    b.iter(|| {
        for _ in 0..10 {
            brd = brd.parallel_next_generation();
        }
    });
}

// In-place (double-buffered, no allocation) generation advances.

/// The dedicated B3/S23 fast path (`bit1 & !c02 & (bit0 | center)`).
#[bench]
fn bench_bitboard_step(b: &mut Bencher) {
    let mut brd = BitBoard::new(1000, 1000).random();

    b.iter(|| {
        for _ in 0..10 {
            brd.step();
        }
    });
}

/// The general per-neighbor-count path, used for any non-Conway rule
/// (Day & Night here). This is the cost a custom `--rules` pays vs the fast path.
#[bench]
fn bench_bitboard_step_general(b: &mut Bencher) {
    let mut brd =
        BitBoard::new_with_rules(1000, 1000, &Rules::from_str("B368/S245").unwrap()).random();

    b.iter(|| {
        for _ in 0..10 {
            brd.step();
        }
    });
}

#[cfg(feature = "unstable")]
#[bench]
fn bench_bitboard_simd(b: &mut Bencher) {
    let mut simd = SimdBoard::from(&Board::new(1000, 1000).random());

    b.iter(|| {
        for _ in 0..10 {
            simd.step();
        }
    });
}

#[bench]
fn bench_bitboard_population(b: &mut Bencher) {
    let brd = BitBoard::new(1000, 1000).random();
    b.iter(|| brd.population());
}

#[bench]
fn bench_vecbool_population(b: &mut Bencher) {
    let brd = Board::new(1000, 1000).random();
    b.iter(|| brd.population());
}
