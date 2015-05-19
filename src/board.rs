use std::fmt;
use rand::{thread_rng, Rng};
use std::sync::{Arc, RwLock, Barrier};
use std::iter::repeat;
use std::num::Wrapping;
use threadpool::ThreadPool;
use num_cpus;

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
    pool: ThreadPool,
    size: usize
}

impl WorkerPool {
    pub fn new(size: usize) -> WorkerPool {
        WorkerPool {
            pool: ThreadPool::new(size),
            size: size
        }
    }

    pub fn new_with_default_size() -> WorkerPool {
        WorkerPool::new(num_cpus::get())
    }
}

struct BoardAdvancer {
    board: Board,
    next_cells: RwLock<Vec<Vec<bool>>>
}

impl BoardAdvancer {
    fn new(board: &Board, num_tasks: usize) -> BoardAdvancer {
        BoardAdvancer {
            board: board.clone(),
            next_cells: RwLock::new(repeat(vec![]).take(num_tasks).collect()),
        }
    }

    fn advance(board: &Board, workers: &mut WorkerPool) -> Vec<bool> {
        let shared_board = Arc::new(BoardAdvancer::new(board, workers.size));
        let length = board.len();
        let all_tasks: Vec<usize> = (0..length).collect();
        let tasks: Vec<&[usize]> = all_tasks
            .chunks((length + workers.size - 1) / workers.size)
            .collect();
        let barrier = Arc::new(Barrier::new(tasks.clone().len()+1));

        for (i, task) in tasks.iter().enumerate() {
            let task_board = shared_board.clone();
            let task = task.to_vec();
            let done = barrier.clone();

            workers.pool.execute(move || {
                let task_values = task.iter().map(|&idx|
                    task_board.board.successor_cell(idx)
                ).collect::<Vec<bool>>();
                if let Ok(mut task_results) = task_board.next_cells.write() {
                    task_results[i] = task_values;
                }
                done.wait();
            });

        };

        barrier.wait();
        let next_board = shared_board.next_cells.read().unwrap();
        next_board.concat()
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
        let Wrapping(x_1) = Wrapping(x) - Wrapping(1);
        let Wrapping(y_1) = Wrapping(y) - Wrapping(1);
        let neighbors = [
            self.cell_live(x_1, y_1), self.cell_live(x, y_1), self.cell_live(x+1, y_1),
            self.cell_live(x_1, y+0),                         self.cell_live(x+1, y+0),
            self.cell_live(x_1, y+1), self.cell_live(x, y+1), self.cell_live(x+1, y+1),
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

    pub fn toggle(&self, x:usize, y:usize) -> Board {
        if x < self.rows && y < self.cols {
            let mut board = self.board.clone();
            board[x * self.cols + y] = !board[x * self.cols + y];
            self.next_board(board)
        } else {
            self.clone()
        }
    }

    pub fn clear(self) -> Board {
        Board::new(self.rows, self.cols)
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

    pub fn cells(&self) -> Vec<(usize, usize, bool)> {
        self.board.iter()
            .enumerate()
            .map(|(i, v)| (i % self.cols, i / self.cols, *v))
            .collect()
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
