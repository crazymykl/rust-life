#![cfg_attr(all(test, feature = "unstable"), feature(test))]

extern crate rand;
extern crate rayon;

#[cfg(all(test, feature = "unstable"))]
mod benchmarks;

mod board;

#[cfg(not(test))]
mod gui;

#[cfg(not(test))]
fn main() {
    gui::main();
}
