use std::fmt;
use std::str::FromStr;
use rand::{thread_rng, Rng};
use std::iter::repeat;
use std::num::Wrapping;
use std::sync::Arc;
use rayon::prelude::*;

const LIVE_CELL: char = '@';
const DEAD_CELL: char = '.';

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Board {
    board: Vec<bool>,
    survive: Arc<Vec<usize>>,
    born: Arc<Vec<usize>>,
    rows: usize,
    cols: usize
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
                born   : Arc::new(born),
                survive: Arc::new(survive),
                rows   : rows,
                cols   : cols }
    }

    fn len(&self) -> usize {
        self.rows * self.cols
    }

    fn next_board(&self, new_board: Vec<bool>) -> Board {
        assert_eq!(new_board.len(), self.len());

        Board { board  : new_board,
                born   : self.born.clone(),
                survive: self.survive.clone(),
                rows   : self.rows,
                cols   : self.cols }
    }

    pub fn random(&self) -> Board {
        let brd = thread_rng().gen_iter().take(self.len()).collect();

        self.next_board(brd)
    }

    #[allow(dead_code)]
    pub fn next_generation(&self) -> Board {
        let new_brd = (0..self.len()).map(|cell| self.successor_cell(cell)).collect();

        self.next_board(new_brd)
    }

    pub fn parallel_next_generation(&self) -> Board {
        let new_brd = (0..self.len())
            .into_par_iter()
            .map(|cell| self.successor_cell(cell))
            .collect();

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
            self.cell_live(x_1, y  ),                         self.cell_live(x+1, y  ),
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

        write!(f, "{}", rows.join("\n"))
    }
}

pub struct ParseBoardErr();

impl FromStr for Board {
    type Err = ParseBoardErr;

    fn from_str(string: &str) -> Result<Board, ParseBoardErr> {
        let rows: Vec<&str> = string.split_terminator('\n').collect();
        let (row_cnt, col_cnt) = (rows[0].len(), rows.len());

        if rows.iter().any(|x| x.len() != row_cnt) { return Err(ParseBoardErr()) };

        let chars: String = rows.concat();

        let brd: Option<Vec<bool>> = chars.chars().map(|c| match c {
                LIVE_CELL => Some(true),
                DEAD_CELL => Some(false),
                _         => None
            }).collect();

        match brd {
            Some(board) => Ok(Board::new(row_cnt, col_cnt).next_board(board)),
            None        => Err(ParseBoardErr())
        }
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
    Board::from_str(TEST_BOARDS[n]).ok().unwrap()
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
    assert_eq!(testing_board(1).parallel_next_generation(), testing_board(2));
}

#[test]
fn test_clear() {
    let brd = testing_board(0);

    assert!(brd.clear().cells().iter().all(|&(_, _, x)| !x));
}

#[test]
fn test_random() {
    let (brd, brd2) = (testing_board(0).random(), testing_board(0).random());

    assert!(brd != brd2);
}

#[test]
fn test_toggle() {
    let brd = testing_board(0);

    assert_eq!(brd.toggle(0, 0).toggle(0, 0), brd);
}
