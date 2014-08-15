mod board;

#[cfg(not(test))]
fn main() {
  let mut brd = board::Board::new(65, 248).random();
  let mut timer = std::io::Timer::new().unwrap();

  let periodic = timer.periodic(std::time::Duration::milliseconds(64));
  loop {
    println!("\x1b[H\x1b[2J{}", brd);
    periodic.recv();
    brd = brd.parallel_next_generation();
  }
}
