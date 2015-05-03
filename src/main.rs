#![feature(std_misc, test)]

extern crate threadpool;
extern crate rand;
extern crate num_cpus;
extern crate snooze;

use std::time::duration::Duration;

mod board;

#[cfg(not(test))]
fn main() {
    let mut snooze = snooze::Snooze::new(Duration::milliseconds(64)).unwrap();
    let mut brd = board::Board::new(65, 248).random();
    let ref mut worker_pool = board::WorkerPool::new_with_default_size();
    loop {
        println!("\x1b[H\x1b[2J{}", brd);
        let _ = snooze.wait();
        brd = brd.parallel_next_generation(worker_pool);
    }
}
