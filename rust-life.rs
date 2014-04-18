extern crate rand;

#[cfg(test)]
extern crate test;

use std::vec;
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
  let mut brd = Board::new(65,250).random();
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
  board: Vec<Vec<bool>>,
  rows: uint,
  cols: uint
}

impl Board {
  fn new(rows: uint, cols: uint) -> Board {
    let new_board = Vec::from_elem(rows, Vec::from_elem(cols, false));
    Board { board: new_board, rows: rows, cols: cols }
  }

  fn random(&self) -> Board {
    let board = Vec::from_fn(self.rows, |_| {
      Vec::from_slice(task_rng().gen_vec(self.cols))
    });

    Board { board: board, rows: self.rows, cols: self.cols }
  }

  fn next_generation(&self) -> Board {
    let new_brd = Vec::from_fn(self.rows, |row| {
      Vec::from_fn(self.cols, |col| self.successor(col, row))
    });
    Board { board: new_brd, rows: self.rows, cols: self.cols }
  }

  fn cell_live(&self, x: uint, y: uint) -> bool {
    if x >= self.cols || y >= self.rows {
      false
    } else {
      *self.board.get(y).get(x)
    }
  }

  fn living_neighbors(&self, x: uint, y: uint) -> uint {
    let neighbors = [
      self.cell_live(x-1, y-1), self.cell_live(x, y-1), self.cell_live(x+1, y-1),
      self.cell_live(x-1, y+0),                         self.cell_live(x+1, y+0),
      self.cell_live(x-1, y+1), self.cell_live(x, y+1), self.cell_live(x+1, y+1),
    ];
    neighbors.iter().count(|x| *x)
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
    fn process_rows(string: &str) -> Option<Vec<Vec<bool>>> {
      let mut rows = string.split_terminator('\n').peekable();
      let col_count = match rows.peek() {
        Some(cols) => Some(cols.len()),
        None       => None
      };
      col_count.and(option::collect(rows.map(|row| {
        let len = row.len();
        if len != 0 && Some(len) == col_count {
          process_cells(row)
        } else {
          None
        }
      })))
    }

    fn process_cells(row: &str) -> Option<Vec<bool>> {
      option::collect(row.chars().map(process_cell))
    }

    fn process_cell(cell: char) -> Option<bool> {
      match cell {
        LIVE_CELL => Some(true),
        DEAD_CELL => Some(false),
        _         => None
      }
    }

    let board = process_rows(string);
    match board {
      Some(brd) => Some(Board { board: brd.clone(),
                                rows : brd.len(),
                                cols : brd.get(0).len() }),
      None      => None
    }
  }
}

impl fmt::Show for Board {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {

    fn row_to_str(row: &Vec<bool>) -> ~str {
      let chars: ~[char] = row.iter().map(|cell|
        if *cell {LIVE_CELL} else {DEAD_CELL}
      ).collect();
      str::from_chars(chars)
    }

    let rows: ~[~str] = self.board.iter().map(|row|
      row_to_str(row)
    ).collect();

    write!(f.buf, "{}", rows.connect("\n"))
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
