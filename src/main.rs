#![cfg_attr(all(test, feature = "unstable"), feature(test))]

extern crate threadpool;
extern crate rand;
extern crate num_cpus;

#[cfg(all(test, feature = "unstable"))]
mod benchmarks;

mod board;

#[cfg(not(test))]
mod gui;

#[cfg(not(test))]
fn main() {
    gui::main();
}
