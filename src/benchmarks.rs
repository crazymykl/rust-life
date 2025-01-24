extern crate test;

use self::test::Bencher;
use crate::board::Board;
use assert_cmd::Command;

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

#[cfg(feature = "rayon")]
#[bench]
fn bench_ten_parallel_generations(b: &mut Bencher) {
    let mut brd = Board::new(200, 200).random();

    b.iter(|| {
        for _ in 0..10 {
            brd = brd.parallel_next_generation();
        }
    });
}

fn bin() -> Command {
    Command::cargo_bin("rust-life").unwrap()
}

#[bench]
fn bench_ten_cli_generations(b: &mut Bencher) {
    b.iter(|| {
        bin()
            .args([
                #[cfg(feature = "gui")]
                "--no-gui",
                "-G10",
            ])
            .assert()
            .success();
    });
}

#[cfg(feature = "gui")]
#[bench]
fn bench_ten_gui_generations(b: &mut Bencher) {
    b.iter(|| {
        bin().args(["-G10", "-x"]).assert().success();
    });
}
