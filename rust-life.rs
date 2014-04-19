extern crate rand;
extern crate sync;

#[cfg(test)]
extern crate test;

use std::slice;
use std::str;
use std::fmt;
use std::option;
use std::io::Timer;
use rand::{task_rng, Rng};

#[cfg(test)]
use test::BenchHarness;

static LIVE_CELL: char = '@';
static DEAD_CELL: char = '.';

fn main() {
  let mut brd = Board::new(65, 250).random();
  let mut timer = Timer::new().unwrap();

  let periodic = timer.periodic(64);
  loop {
    println!("\x1b[H\x1b[2J{}", brd);
    periodic.recv();
    brd = brd.next_generation();
  }
}

#[deriving(Eq)]
struct Board {
  board: ~[bool],
  rows: uint,
  cols: uint
}

impl Board {
  fn new(rows: uint, cols: uint) -> Board {
    let new_board = slice::from_elem(rows * cols, false);
    Board { board: new_board, rows: rows, cols: cols }
  }

  fn len(&self) -> uint {
    self.rows * self.cols
  }

  fn random(&self) -> Board {
    let board = task_rng().gen_vec(self.len());

    Board { board: board, rows: self.rows, cols: self.cols }
  }

  fn next_generation(&self) -> Board {
    let new_brd = slice::from_fn(self.len(), |cell| self.successor_cell(cell));
    Board { board: new_brd, rows: self.rows, cols: self.cols }
  }

  fn cell_live(&self, x: uint, y: uint) -> bool {
    if x >= self.cols || y >= self.rows {
      false
    } else {
      self.board[y * self.cols + x]
    }
  }

  fn living_neighbors(&self, x: uint, y: uint) -> uint {
    let neighbors = [
      self.cell_live(x-1, y-1), self.cell_live(x, y-1), self.cell_live(x+1, y-1),
      self.cell_live(x-1, y+0),                         self.cell_live(x+1, y+0),
      self.cell_live(x-1, y+1), self.cell_live(x, y+1), self.cell_live(x+1, y+1),
    ];
    neighbors.iter().count(|&x| x)
  }

  fn successor_cell(&self, cell:uint) -> bool {
    self.successor(cell % self.cols, cell / self.cols)
  }

  fn successor(&self, x:uint, y:uint) -> bool {
    let neighbors = self.living_neighbors(x, y);
    if self.cell_live(x, y) {
      neighbors == 2 || neighbors == 3
    } else {
      neighbors == 3
    }
  }

  fn from_str(string: &str) -> Option<Board> {
    let rows: ~[&str] = string.split_terminator('\n').collect();
    let (row_cnt, col_cnt) = (rows[0].len(), rows.len());

    if rows.iter().any(|x| x.len() != row_cnt) { return None; };

    let brd = option::collect(
      rows.concat().chars().map(|c| match c {
        LIVE_CELL => Some(true),
        DEAD_CELL => Some(false),
        _         => None
      })
    );

    match brd {
      Some(board) => Some(Board { board: board, rows: row_cnt, cols: col_cnt }),
      None        => None
    }
  }
}

impl fmt::Show for Board {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {

    fn row_to_str(row: &[bool]) -> ~str {
      let chars: ~[char] = row.iter().map(|&cell|
        if cell {LIVE_CELL} else {DEAD_CELL}
      ).collect();
      str::from_chars(chars)
    }

    let rows: ~[~str] = self.board.as_slice().chunks(self.cols).map(|row|
      row_to_str(row)
    ).collect();

    write!(f.buf, "{}", rows.connect("\n"))
  }
}

impl Clone for Board {
  fn clone(&self) -> Board {
    Board { board: self.board.clone(), rows: self.rows, cols: self.cols }
  }
}

#[cfg(test)]
fn testing_board(n: int) -> Board {
  let brds = [
    ".@.\n" +
    ".@@\n" +
    ".@@\n"
    ,
    "...\n" +
    "@@@\n" +
    "...\n"
    ,
    ".@.\n" +
    ".@.\n" +
    ".@.\n" ];
  Board::from_str(brds[n]).unwrap()
}

#[test]
fn test_cell_live() {
  let brd = testing_board(0);
  assert!(!brd.cell_live(0, 0));
  assert!(brd.cell_live(2, 2));
}

#[test]
fn test_live_count() {
  let brd = testing_board(0);
  assert!(brd.living_neighbors(0, 0) == 2);
  assert!(brd.living_neighbors(2, 2) == 3);
}

#[test]
fn test_next_generation() {
  assert!(testing_board(1).next_generation() == testing_board(2))
}

#[bench]
fn bench_random(b: &mut BenchHarness) {
  let brd = Board::new(200,200);
  b.iter(|| {brd.random();})
}

#[bench]
fn bench_hundred_generations(b: &mut BenchHarness) {
  let mut brd = Board::new(200,200).random();
  b.iter(|| {
    for _ in range(0,100) { brd = brd.next_generation() }
  });
}
