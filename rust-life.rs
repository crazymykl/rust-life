extern crate rand;
extern crate sync;

#[cfg(test)]
extern crate test;

use std::{cmp, str, fmt, rt, option, io};
use rand::{task_rng, Rng};
use sync::{Arc, Future};

#[cfg(test)]
use test::Bencher;

static LIVE_CELL: char = '@';
static DEAD_CELL: char = '.';

fn main() {
  let mut brd = Board::new(65, 248).random();
  let mut timer = io::Timer::new().unwrap();

  let periodic = timer.periodic(64);
  loop {
    println!("\x1b[H\x1b[2J{}", brd);
    periodic.recv();
    brd = brd.parallel_next_generation();
  }
}

#[deriving(Eq, Clone)]
struct Board {
  board: Vec<bool>,
  survive: Vec<uint>,
  born: Vec<uint>,
  rows: uint,
  cols: uint
}

impl Board {
  fn new(rows: uint, cols: uint) -> Board {
    let born = vec![3];
    let survive = vec![2, 3];

    Board::new_with_custom_rules(rows, cols, born, survive)
  }

  fn new_with_custom_rules(rows: uint, cols: uint, born: Vec<uint>, survive: Vec<uint>) -> Board {
    let new_board = Vec::from_elem(rows * cols, false);

    Board { board  : new_board,
            born   : born,
            survive: survive,
            rows   : rows,
            cols   : cols }
  }

  fn len(&self) -> uint {
    self.rows * self.cols
  }

  fn next_board(&self, new_board: Vec<bool>) -> Board {
    assert!(new_board.len() == self.len());

    Board { board  : new_board,
            born   : self.born.clone(),
            survive: self.survive.clone(),
            rows   : self.rows,
            cols   : self.cols }
  }

  fn random(&self) -> Board {
    let board = task_rng().gen_vec(self.len());

    self.next_board(board)
  }

  fn next_generation(&self) -> Board {
    let new_brd = Vec::from_fn(self.len(), |cell| self.successor_cell(cell));

    self.next_board(new_brd)
  }

  fn parallel_next_generation(&self) -> Board {
    let length = self.len();
    let num_tasks = cmp::min(rt::default_sched_threads(), length);
    let shared_brd = Arc::new(self.clone());
    let all_tasks: Vec<uint> = range(0, length).collect();
    let tasks: Vec<&[uint]> = all_tasks.as_slice().chunks(length / num_tasks).collect();

    fn future_batch(task_brd: Arc<Board>, task: ~[uint]) -> Future<Vec<bool>> {
      Future::spawn(proc()
        task.iter().map(|&idx| task_brd.successor_cell(idx)).collect()
      )
    }

    let future_batches: Vec<Future<Vec<bool>>> = tasks.move_iter().map(|task| {
      future_batch(shared_brd.clone(), task.to_owned())
    }).collect();

    let mut new_brd: Vec<bool> = Vec::with_capacity(length);

    for b in future_batches.move_iter() {
      new_brd.push_all_move(b.unwrap());
    }

    self.next_board(new_brd)
  }

  fn cell_live(&self, x: uint, y: uint) -> bool {
    !(x >= self.cols || y >= self.rows) && *self.board.get(y * self.cols + x)
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
      self.survive.contains(&neighbors)
    } else {
      self.born.contains(&neighbors)
    }
  }

  fn from_str(string: &str) -> Option<Board> {
    let rows: Vec<&str> = string.split_terminator('\n').collect();
    let (row_cnt, col_cnt) = (rows.get(0).len(), rows.len());

    if rows.iter().any(|x| x.len() != row_cnt) { return None; };

    let brd = option::collect(
      rows.concat().chars().map(|c| match c {
        LIVE_CELL => Some(true),
        DEAD_CELL => Some(false),
        _         => None
      })
    );

    match brd {
      Some(board) => Some(Board::new(row_cnt, col_cnt).next_board(board)),
      None        => None
    }
  }
}

impl fmt::Show for Board {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {

    fn row_to_str(row: &[bool]) -> ~str {
      let chars: Vec<char> = row.iter().map(|&cell|
        if cell {LIVE_CELL} else {DEAD_CELL}
      ).collect();
      str::from_chars(chars.as_slice())
    }

    let rows: Vec<~str> = self.board.as_slice().chunks(self.cols).map(|row|
      row_to_str(row)
    ).collect();

    write!(f, "{}", rows.connect("\n"))
  }
}

#[cfg(test)]
static test_boards: [&'static str, ..3] = [
  ".@.\n.@@\n.@@",
  "...\n@@@\n...",
  ".@.\n.@.\n.@."
];

#[cfg(test)]
fn testing_board(n: uint) -> Board {
  Board::from_str(test_boards[n]).unwrap()
}

#[test]
fn test_board_str_conversion() {
  assert_eq!(testing_board(0).to_str(), test_boards[0].to_owned());
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
  assert_eq!(brd.living_neighbors(0, 0), 2);
  assert_eq!(brd.living_neighbors(2, 2), 3);
}

#[test]
fn test_next_generation() {
  assert_eq!(testing_board(1).next_generation(), testing_board(2));
}

#[test]
fn test_parallel_next_generation() {
  assert_eq!(testing_board(1).parallel_next_generation(), testing_board(2));
}

#[bench]
fn bench_random(b: &mut Bencher) {
  let brd = Board::new(200,200);
  b.iter(|| brd.random());
}

#[bench]
fn bench_ten_generations(b: &mut Bencher) {
  let mut brd = Board::new(200,200).random();
  b.iter(||
    for _ in range(0,10) { brd = brd.next_generation() }
  );
}

#[bench]
fn bench_ten_parallel_generations(b: &mut Bencher) {
  let mut brd = Board::new(200,200).random();
  b.iter(||
    for _ in range(0,10) { brd = brd.parallel_next_generation() }
  );
}
