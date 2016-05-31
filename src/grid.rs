//! Functions for computing properties of the terminal grid

use std::collections::VecDeque;

use std::ops::{Index, IndexMut, Deref, DerefMut};

use term::Cursor;
use ::Rgb;

/// Calculate the number of cells for an axis
pub fn num_cells_axis(cell_width: u32, cell_sep: i32, screen_width: u32) -> u32 {
    println!("num_cells_axis(cell_width: {}, cell_sep: {}, screen_width: {}",
             cell_width, cell_sep, screen_width);
    ((screen_width as i32 - cell_sep) as f64 / (cell_width as i32 + cell_sep) as f64) as u32
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub c: char,
    pub fg: Rgb,
    pub bg: Rgb,
}

impl Cell {
    pub fn new(c: char) -> Cell {
        Cell {
            c: c.into(),
            bg: Default::default(),
            fg: Default::default(),
        }
    }
}

/// Represents the terminal display contents
#[derive(Clone)]
pub struct Grid {
    /// Rows in the grid. Each row holds a list of cells corresponding to the columns in that row.
    raw: VecDeque<Row>,

    /// Number of columns
    cols: usize,

    /// Number of rows.
    ///
    /// Invariant: rows is equivalent to cells.len()
    rows: usize,
}

impl Grid {
    pub fn new(rows: usize, cols: usize) -> Grid {
        let mut raw = VecDeque::with_capacity(rows);
        for _ in 0..raw.capacity() {
            raw.push_back(Row::new(cols));
        }

        Grid {
            raw: raw,
            cols: cols,
            rows: rows,
        }
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn feed(&mut self) {
        // do the borrowck dance
        let row = self.raw.pop_front().unwrap();
        self.raw.push_back(row);
    }

    pub fn unfeed(&mut self) {
        // do the borrowck dance
        let row = self.raw.pop_back().unwrap();
        self.raw.push_front(row);
    }

    pub fn clear(&mut self) {
        for row in self.raw.iter_mut() {
            for cell in row.iter_mut() {
                cell.c = ' ';
            }
        }
    }
}

impl Index<usize> for Grid {
    type Output = Row;

    #[inline]
    fn index<'a>(&'a self, index: usize) -> &'a Row {
        &self.raw[index]
    }
}

impl IndexMut<usize> for Grid {
    #[inline]
    fn index_mut<'a>(&'a mut self, index: usize) -> &'a mut Row {
        &mut self.raw[index]
    }
}

impl Index<Cursor> for Grid {
    type Output = Cell;

    #[inline]
    fn index<'a>(&'a self, cursor: Cursor) -> &'a Cell {
        &self.raw[cursor.y as usize][cursor.x as usize]
    }
}

impl IndexMut<Cursor> for Grid {
    #[inline]
    fn index_mut<'a>(&'a mut self, cursor: Cursor) -> &'a mut Cell {
        &mut self.raw[cursor.y as usize][cursor.x as usize]
    }
}

/// A row in the grid
#[derive(Debug, Clone)]
pub struct Row(Vec<Cell>);

impl Row {
    pub fn new(columns: usize) -> Row {
        Row(vec![Cell::new(' '); columns])
    }

    pub fn cols(&self) -> usize {
        self.0.len()
    }
}

impl Deref for Row {
    type Target = Vec<Cell>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Row {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Index<usize> for Row {
    type Output = Cell;

    #[inline]
    fn index<'a>(&'a self, index: usize) -> &'a Cell {
        &self.0[index]
    }
}

impl IndexMut<usize> for Row {
    #[inline]
    fn index_mut<'a>(&'a mut self, index: usize) -> &'a mut Cell {
        &mut self.0[index]
    }
}
