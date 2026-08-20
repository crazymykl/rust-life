#![feature(test)]

extern crate test;

use self::test::Bencher;
use assert_cmd::{Command, cargo};

fn bin() -> Command {
    Command::new(cargo::cargo_bin!("rust-life"))
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
