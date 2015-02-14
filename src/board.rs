#[cfg(test)]
extern crate test;

use std::{fmt, rt};
use std::rand::{thread_rng, Rng};
use std::sync::{Arc, TaskPool, RwLock, Semaphore};
use std::iter::repeat;

#[cfg(test)]
use self::test::Bencher;

const LIVE_CELL: char = '@';
const DEAD_CELL: char = '.';

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Board {
  board: Vec<bool>,
  survive: Vec<usize>,
  born: Vec<usize>,
  rows: usize,
  cols: usize
}

pub struct WorkerPool {
  pool: TaskPool,
  size: usize
}

impl WorkerPool {
  pub fn new(size: usize) -> WorkerPool {
    WorkerPool {
      pool: TaskPool::new(size),
      size: size
    }
  }

  pub fn new_with_default_size() -> WorkerPool {
    WorkerPool::new(rt::default_sched_threads())
  }
}

struct FutureBoard {
  board: Vec<Vec<bool>>,
  tasks_done: usize,
}

impl FutureBoard {
  fn cells(&self) -> Vec<bool> {
    self.board.concat()
  }
}

struct BoardAdvancer {
  board: Board,
  next_board: RwLock<FutureBoard>,
  done: Semaphore
}

impl BoardAdvancer {
  fn new(board: &Board, num_tasks: usize) -> BoardAdvancer {
    BoardAdvancer {
      board: board.clone(),
      next_board: RwLock::new(FutureBoard {
        board: repeat(vec![]).take(num_tasks).collect(),
        tasks_done: 0
      }),
      done: Semaphore::new(0)
    }
  }

  fn advance(board: &Board, workers: &mut WorkerPool) -> Vec<bool> {
    let shared_board = Arc::new(BoardAdvancer::new(board, workers.size));
    let length = board.len();
    let all_tasks: Vec<usize> = (0..length).collect();
    let tasks: Vec<&[usize]> = all_tasks.chunks((length as usize + workers.size - 1) / workers.size).collect();
    let task_count = tasks.clone().len();

    for (i, task) in tasks.iter().enumerate() {
      let task_board = shared_board.clone();
      let task = task.to_vec();

      workers.pool.execute(move || {
        let task_values = task.iter().map(|&idx|
          task_board.board.successor_cell(idx)
        ).collect::<Vec<bool>>();
        let mut task_results = task_board.next_board.write().unwrap();
        task_results.board[i] = task_values;
        task_results.tasks_done += 1;
        if task_results.tasks_done == task_count { task_board.done.release(); }
      });

    };
    shared_board.done.acquire();
    let next_board = shared_board.next_board.read().unwrap();
    next_board.cells()
  }


}

impl Board {
  pub fn new(rows: usize, cols: usize) -> Board {
    let born = vec![3];
    let survive = vec![2, 3];

    Board::new_with_custom_rules(rows, cols, born, survive)
  }

  fn new_with_custom_rules(rows: usize, cols: usize, born: Vec<usize>, survive: Vec<usize>) -> Board {
    let new_board = repeat(false).take(rows * cols).collect();

    Board { board  : new_board,
            born   : born,
            survive: survive,
            rows   : rows,
            cols   : cols }
  }

  fn len(&self) -> usize {
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

  pub fn random(&self) -> Board {
    let board = thread_rng().gen_iter::<bool>().take(self.len()).collect();

    self.next_board(board)
  }

  fn next_generation(&self) -> Board {
    let new_brd = (0..self.len()).map(|cell| self.successor_cell(cell)).collect();

    self.next_board(new_brd)
  }

  pub fn parallel_next_generation(&self, workers: &mut WorkerPool) -> Board {
    let new_brd = BoardAdvancer::advance(self, workers);

    self.next_board(new_brd)
  }

  fn cell_live(&self, x: usize, y: usize) -> bool {
    !(x >= self.cols || y >= self.rows) && self.board[y * self.cols + x]
  }

  fn living_neighbors(&self, x: usize, y: usize) -> usize {
    let neighbors = [
      self.cell_live(x-1, y-1), self.cell_live(x, y-1), self.cell_live(x+1, y-1),
      self.cell_live(x-1, y+0),                         self.cell_live(x+1, y+0),
      self.cell_live(x-1, y+1), self.cell_live(x, y+1), self.cell_live(x+1, y+1),
    ];
    neighbors.iter().filter(|&x| *x).count()
  }

  fn successor_cell(&self, cell: usize) -> bool {
    self.successor(cell % self.cols, cell / self.cols)
  }

  fn successor(&self, x:usize, y:usize) -> bool {
    let neighbors = self.living_neighbors(x, y);
    if self.cell_live(x, y) {
      self.survive.contains(&neighbors)
    } else {
      self.born.contains(&neighbors)
    }
  }

  fn from_str(string: &str) -> Option<Board> {
    let rows: Vec<&str> = string.split_terminator('\n').collect();
    let (row_cnt, col_cnt) = (rows[0].len(), rows.len());

    if rows.iter().any(|x| x.len() != row_cnt) { return None; };

    let chars: String = rows.concat();

    let brd: Option<Vec<bool>> = chars.chars().map(|c| match c {
        LIVE_CELL => Some(true),
        DEAD_CELL => Some(false),
        _         => None
      }).collect();

    match brd {
      Some(board) => Some(Board::new(row_cnt, col_cnt).next_board(board)),
      None        => None
    }
  }
}

impl fmt::Display for Board {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {

    fn row_to_str(row: &[bool]) -> String {
      row.iter().map(|&cell|
        if cell {LIVE_CELL} else {DEAD_CELL}
      ).collect()
    }

    let rows: Vec<String> = self.board.chunks(self.cols).map(|row|
      row_to_str(row)
    ).collect();

    write!(f, "{}", rows.connect("\n"))
  }
}

#[cfg(test)]
const TEST_BOARDS: [&'static str; 3] = [
  ".@.\n.@@\n.@@",
  "...\n@@@\n...",
  ".@.\n.@.\n.@."
];

#[cfg(test)]
fn testing_board(n: usize) -> Board {
  Board::from_str(TEST_BOARDS[n]).unwrap()
}

#[test]
fn test_board_str_conversion() {
  assert_eq!(format!("{}", testing_board(0)), TEST_BOARDS[0]);
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
  let ref mut workers = WorkerPool::new_with_default_size();

  assert_eq!(testing_board(1).parallel_next_generation(workers), testing_board(2));
}

#[bench]
fn bench_random(b: &mut Bencher) {
  let brd = Board::new(200,200);
  b.iter(|| brd.random());
}

#[bench]
fn bench_ten_generations(b: &mut Bencher) {
  let mut brd = Board::new(200,200).random();
  b.iter(|| for _ in (0..10) { brd = brd.next_generation(); });
}

#[bench]
fn bench_ten_parallel_generations(b: &mut Bencher) {
  let mut brd = Board::new(200,200).random();
  let ref mut workers = WorkerPool::new_with_default_size();

  b.iter(|| for _ in (0..10) { brd = brd.parallel_next_generation(workers); });
}
