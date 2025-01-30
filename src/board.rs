use rand::{distributions::Standard, thread_rng, Rng};
#[cfg(feature = "rayon")]
use rayon::prelude::*;
use std::cmp::max;
use std::error::Error;
use std::fmt;
use std::iter::{repeat, repeat_n};
use std::str::FromStr;
use std::sync::Arc;

const LIVE_CELL: char = '@';
const DEAD_CELL: char = '.';

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Board {
    board: Vec<bool>,
    survive: Arc<Vec<usize>>,
    born: Arc<Vec<usize>>,
    rows: usize,
    cols: usize,
    generation: usize,
}

impl Board {
    pub fn new(rows: usize, cols: usize) -> Board {
        let born = vec![3];
        let survive = vec![2, 3];

        Board::new_with_custom_rules(rows, cols, born, survive)
    }

    fn new_with_custom_rules(
        rows: usize,
        cols: usize,
        born: Vec<usize>,
        survive: Vec<usize>,
    ) -> Board {
        let new_board = repeat_n(false, rows * cols).collect();

        Board {
            board: new_board,
            born: Arc::new(born),
            survive: Arc::new(survive),
            rows,
            cols,
            generation: 0,
        }
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn generation(&self) -> usize {
        self.generation
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.rows * self.cols
    }

    #[allow(dead_code)]
    pub fn population(&self) -> usize {
        self.iter().filter(|&&x| x).count()
    }

    fn next_board(&self, new_board: Vec<bool>) -> Board {
        assert_eq!(new_board.len(), self.len());

        self.resized_next_board(new_board, self.rows, self.cols)
    }

    fn resized_next_board(&self, new_board: Vec<bool>, rows: usize, cols: usize) -> Board {
        assert_eq!(new_board.len(), rows * cols);

        Board {
            board: new_board,
            born: Arc::clone(&self.born),
            survive: Arc::clone(&self.survive),
            rows,
            cols,
            generation: self.generation,
        }
    }

    fn next_generation_board(&self, new_board: Vec<bool>) -> Board {
        Board {
            generation: self.generation + 1,
            ..self.next_board(new_board)
        }
    }

    pub fn random(&self) -> Board {
        let brd = thread_rng()
            .sample_iter(&Standard)
            .take(self.len())
            .collect();

        self.next_board(brd)
    }

    #[allow(dead_code)]
    pub fn serial_next_generation(&self) -> Board {
        let new_brd = (0..self.len())
            .map(|cell| self.successor_cell(cell))
            .collect();

        self.next_generation_board(new_brd)
    }

    pub fn next_generation(&self) -> Board {
        #[cfg(feature = "rayon")]
        let next = self.parallel_next_generation();
        #[cfg(not(feature = "rayon"))]
        let next = self.serial_next_generation();
        next
    }

    #[cfg(feature = "rayon")]
    pub fn parallel_next_generation(&self) -> Board {
        let new_brd = (0..self.len())
            .into_par_iter()
            .map(|cell| self.successor_cell(cell))
            .collect();

        self.next_generation_board(new_brd)
    }

    #[cfg(not(feature = "rayon"))]
    pub fn parallel_next_generation(&self) -> Board {
        unimplemented!("Need 'rayon' feature for parallelism")
    }

    fn cell_live(&self, x: usize, y: usize) -> bool {
        !(x >= self.cols || y >= self.rows) && self.board[y * self.cols + x]
    }

    fn living_neighbors(&self, x: usize, y: usize) -> usize {
        let (x_1, y_1) = (x.wrapping_sub(1), y.wrapping_sub(1));
        #[rustfmt::skip]
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

    fn successor(&self, x: usize, y: usize) -> bool {
        let neighbors = self.living_neighbors(x, y);
        if self.cell_live(x, y) {
            self.survive.contains(&neighbors)
        } else {
            self.born.contains(&neighbors)
        }
    }

    pub fn toggle(&self, x: usize, y: usize) -> Board {
        if x < self.rows && y < self.cols {
            let mut board = self.board.clone();
            board[x * self.cols + y] = !board[x * self.cols + y];
            self.next_board(board)
        } else {
            self.clone()
        }
    }

    pub fn clear(&self) -> Board {
        Board::new(self.rows, self.cols)
    }

    pub fn pad(&self, top: isize, right: isize, bottom: isize, left: isize) -> Board {
        let new_cell_values = repeat(false);
        let (rows, cols) = (
            max(0, top + self.rows as isize + bottom) as usize,
            max(0, left + self.cols as isize + right) as usize,
        );
        let dst_cells = new_cell_values
            .clone()
            .take(if top > 0 { top as usize * cols } else { 0 })
            .chain(
                self.board
                    .chunks(max(1, self.cols))
                    .skip(if top < 0 { -top as usize } else { 0 })
                    .take(rows)
                    .flat_map(|row| {
                        new_cell_values
                            .clone()
                            .take(if left > 0 { left as usize } else { 0 })
                            .chain(
                                row.iter()
                                    .copied()
                                    .skip(if left < 0 { -left as usize } else { 0 })
                                    .chain(new_cell_values.clone()),
                            )
                            .take(cols)
                    }),
            )
            .chain(new_cell_values.clone().take(if bottom > 0 {
                bottom as usize * cols
            } else {
                0
            }))
            .collect();

        self.resized_next_board(dst_cells, rows, cols)
    }

    pub fn iter(&self) -> std::slice::Iter<bool> {
        self.board.iter()
    }
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fn row_to_str(row: &[bool]) -> String {
            row.iter()
                .map(|&cell| if cell { LIVE_CELL } else { DEAD_CELL })
                .collect()
        }

        let rows: Vec<String> = self.board.chunks(self.cols).map(row_to_str).collect();

        write!(f, "{}", rows.join("\n"))
    }
}

#[derive(Debug, PartialEq)]
pub struct ParseBoardErr(String);

impl Error for ParseBoardErr {}

impl fmt::Display for ParseBoardErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Board {
    type Err = ParseBoardErr;

    fn from_str(string: &str) -> Result<Board, ParseBoardErr> {
        let rows: Vec<&str> = string
            .split_terminator('\n')
            .filter(|row| !row.is_empty())
            .collect();
        let (row_cnt, col_cnt) = (rows.len(), rows.first().map_or(0, |x| x.len()));

        if rows.iter().any(|x| x.len() != col_cnt) {
            return Err(ParseBoardErr("row length mismatch".into()));
        };

        let chars: String = rows.concat();

        let brd: Result<Vec<bool>, ParseBoardErr> = chars
            .chars()
            .map(|c| match c {
                LIVE_CELL => Ok(true),
                DEAD_CELL => Ok(false),
                c => Err(ParseBoardErr(format!("Unexpected '{c}'"))),
            })
            .collect();

        brd.map(|board| Board::new(row_cnt, col_cnt).next_board(board))
    }
}

#[cfg(test)]
#[rustfmt::skip]
const TEST_BOARDS: [&'static str; 9] = [
    ".@.\n.@@\n.@@",
    "...\n@@@\n...",
    ".@.\n.@.\n.@.",
    "@",
    "...\n.@.\n...",
    ".@",
    "@.",
    "@\n.",
    ".\n@",
];

#[cfg(test)]
fn testing_board(n: usize) -> Board {
    Board::from_str(TEST_BOARDS[n]).unwrap()
}

#[cfg(test)]
fn testing_board_generation(n: usize, generation: usize) -> Board {
    Board {
        generation,
        ..testing_board(n)
    }
}

#[test]
fn test_board_str_conversion() {
    assert_eq!(format!("{}", testing_board(0)), TEST_BOARDS[0]);
}

#[test]
fn test_board_str_conversion_error() {
    assert_eq!(
        Board::from_str("!"),
        Err(ParseBoardErr("Unexpected '!'".into()))
    );
    assert_eq!(
        Board::from_str("..\n.").unwrap_err().to_string(),
        "row length mismatch"
    );
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
fn test_population() {
    assert_eq!(testing_board(0).population(), 5);
    assert_eq!(testing_board(1).population(), 3);
    assert_eq!(testing_board(2).population(), 3);
    assert_eq!(testing_board(3).population(), 1);
}

#[test]
fn test_next_generation() {
    assert_eq!(
        testing_board(1).next_generation(),
        testing_board_generation(2, 1)
    );
    assert_eq!(testing_board(1).generation(), 0);
    assert_eq!(testing_board(1).next_generation().generation(), 1);
}

#[test]
fn test_serial_next_generation() {
    assert_eq!(
        testing_board(1).serial_next_generation(),
        testing_board_generation(2, 1)
    );
    assert_eq!(testing_board(1).generation(), 0);
    assert_eq!(testing_board(1).serial_next_generation().generation(), 1);
}

#[cfg(feature = "rayon")]
#[test]
fn test_parallel_next_generation() {
    assert_eq!(
        testing_board(1).parallel_next_generation(),
        testing_board_generation(2, 1)
    );
    assert_eq!(testing_board(1).generation(), 0);
    assert_eq!(testing_board(1).parallel_next_generation().generation(), 1);
}

#[cfg(not(feature = "rayon"))]
#[test]
#[should_panic]
fn test_parallel_next_generation() {
    testing_board(1).parallel_next_generation();
}

#[test]
fn test_clear() {
    let brd = testing_board(0);

    assert!(brd.clear().iter().all(|x| !x));
}

#[test]
fn test_random() {
    let (brd, brd2) = (testing_board(0).random(), testing_board(0).random());

    assert!(brd != brd2);
}

#[test]
fn test_toggle() {
    let brd = testing_board(0);

    assert_ne!(brd.toggle(0, 0), brd);
    assert_eq!(brd.toggle(0, 0).toggle(0, 0), brd);
    // Co-ords outside the board are a no-op
    assert_eq!(brd.toggle(999, 0), brd);
    assert_eq!(brd.toggle(0, 999), brd);
}

#[test]
fn test_pad() {
    assert_eq!(testing_board(3).pad(1, 1, 1, 1), testing_board(4));
    assert_eq!(testing_board(3).pad(0, 0, 0, 1), testing_board(5));
    assert_eq!(testing_board(3).pad(0, 1, 0, 0), testing_board(6));
    assert_eq!(testing_board(3).pad(0, 0, 1, 0), testing_board(7));
    assert_eq!(testing_board(3).pad(1, 0, 0, 0), testing_board(8));
    assert_eq!(testing_board(4).pad(-1, -1, -1, -1), testing_board(3));
    assert_eq!(testing_board(4).pad(-1, -1, -1, 0), testing_board(5));
    assert_eq!(testing_board(4).pad(-1, 0, -1, -1), testing_board(6));
    assert_eq!(testing_board(4).pad(-1, -1, 0, -1), testing_board(7));
    assert_eq!(testing_board(4).pad(0, -1, -1, -1), testing_board(8));
}
