#![feature(std_misc, test, old_io)]

extern crate threadpool;
extern crate rand;
extern crate num_cpus;

mod board;

#[cfg(not(test))]
fn main() {
  let mut brd = board::Board::new(65, 248).random();
  let mut timer = std::old_io::Timer::new().unwrap();
  let ref mut worker_pool = board::WorkerPool::new_with_default_size();
  let periodic = timer.periodic(std::time::Duration::milliseconds(64));
  loop {
    println!("\x1b[H\x1b[2J{}", brd);
    periodic.recv().unwrap();
    brd = brd.parallel_next_generation(worker_pool);
  }
}
