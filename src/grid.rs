//! Functions for computing properties of the terminal grid

use std::ops::{Index, IndexMut, Deref, DerefMut, Range, RangeTo, RangeFrom};
use std::slice::{Iter, IterMut};

use util::Rotate;

use term::{Cursor, DEFAULT_FG, DEFAULT_BG};
use ::Rgb;

#[derive(Clone, Debug)]
pub struct Cell {
    pub c: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub flags: CellFlags,
}

bitflags! {
    pub flags CellFlags: u32 {
        const INVERSE   = 0b00000001,
        const BOLD      = 0b00000010,
        const ITALIC    = 0b00000100,
        const UNDERLINE = 0b00001000,
    }
}

impl Cell {
    pub fn new(c: char) -> Cell {
        Cell {
            c: c.into(),
            bg: Default::default(),
            fg: Default::default(),
            flags: CellFlags::empty(),
        }
    }

    pub fn reset(&mut self) {
        self.c = ' ';
        self.flags = CellFlags::empty();

        // FIXME shouldn't know about term
        self.bg = DEFAULT_BG;
        self.fg = DEFAULT_FG;
    }
}

/// Represents the terminal display contents
#[derive(Clone)]
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
        for _ in 0..rows {
            raw.push(Row::new(cols));
        }

        Grid {
            raw: raw,
            cols: cols,
            rows: rows,
        }
    }

    #[inline]
    pub fn rows(&self) -> Iter<Row> {
        self.raw.iter()
    }

    #[inline]
    pub fn rows_mut(&mut self) -> IterMut<Row> {
        self.raw.iter_mut()
    }

    #[inline]
    pub fn num_rows(&self) -> usize {
        self.raw.len()
    }

    #[inline]
    pub fn num_cols(&self) -> usize {
        self.raw[0].len()
    }

    pub fn scroll(&mut self, region: Range<usize>, positions: isize) {
        self.raw[region].rotate(positions)
    }

    #[inline]
    pub fn clear(&mut self) {
        let region = 0..self.num_rows();
        self.clear_region(region);
    }

    pub fn clear_region(&mut self, region: Range<usize>) {
        for row in self.raw[region].iter_mut() {
            for cell in row.iter_mut() {
                cell.reset();
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

    pub fn cells(&self) -> Iter<Cell> {
        self.0.iter()
    }

    pub fn cells_mut(&mut self) -> IterMut<Cell> {
        self.0.iter_mut()
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

impl Index<RangeFrom<usize>> for Row {
    type Output = [Cell];
    #[inline]
    fn index<'a>(&'a self, index: RangeFrom<usize>) -> &'a [Cell] {
        &self.0[index]
    }
}

impl IndexMut<RangeFrom<usize>> for Row {
    #[inline]
    fn index_mut<'a>(&'a mut self, index: RangeFrom<usize>) -> &'a mut [Cell] {
        &mut self.0[index]
    }
}

impl Index<RangeTo<usize>> for Row {
    type Output = [Cell];
    #[inline]
    fn index<'a>(&'a self, index: RangeTo<usize>) -> &'a [Cell] {
        &self.0[index]
    }
}

impl IndexMut<RangeTo<usize>> for Row {
    #[inline]
    fn index_mut<'a>(&'a mut self, index: RangeTo<usize>) -> &'a mut [Cell] {
        &mut self.0[index]
    }
}
