//! Functions for computing properties of the terminal grid

use std::ops::{Index, IndexMut};

/// Calculate the number of cells for an axis
pub fn num_cells_axis(cell_width: u32, cell_sep: i32, screen_width: u32) -> u32 {
    ((screen_width as i32 + cell_sep) as f64 / (cell_width as i32 - cell_sep) as f64) as u32
}

#[derive(Clone)]
pub struct Cell {
    pub character: Option<String>,
}

impl Cell {
    pub fn new(c: Option<String>) -> Cell {
        Cell {
            character: c,
        }
    }
}

/// Represents the terminal display contents
pub struct Grid {
    /// Rows in the grid. Each row holds a list of cells corresponding to the columns in that row.
    raw: Vec<Row>,

    /// Number of columns
    cols: usize,

    /// Number of rows.
    ///
    /// Invariant: rows is equivalent to cells.len()
    rows: usize,
}

impl Grid {
    pub fn new(rows: usize, cols: usize) -> Grid {
        let mut raw = Vec::with_capacity(rows);
        for _ in 0..raw.capacity() {
            raw.push(Row::new(cols));
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
}

impl Index<usize> for Grid {
    type Output = Row;

    fn index<'a>(&'a self, index: usize) -> &'a Row {
        &self.raw[index]
    }
}

impl IndexMut<usize> for Grid {
    fn index_mut<'a>(&'a mut self, index: usize) -> &'a mut Row {
        &mut self.raw[index]
    }
}

/// A row in the grid
pub struct Row(Vec<Cell>);

impl Row {
    pub fn new(columns: usize) -> Row {
        Row(vec![Cell::new(None); columns])
    }

    pub fn cols(&self) -> usize {
        self.0.len()
    }
}

impl Index<usize> for Row {
    type Output = Cell;

    fn index<'a>(&'a self, index: usize) -> &'a Cell {
        &self.0[index]
    }
}

impl IndexMut<usize> for Row {
    fn index_mut<'a>(&'a mut self, index: usize) -> &'a mut Cell {
        &mut self.0[index]
    }
}
