// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A generic 2d grid implementation optimized for use in a terminal.
//!
//! The current implementation uses a vector of vectors to store cell data.
//! Reimplementing the store as a single contiguous vector may be desirable in
//! the future. Rotation and indexing would need to be reconsidered at that
//! time; rotation currently reorganize Vecs in the lines Vec, and indexing with
//! ranges is currently supported.

use std::cmp::Ordering;
use std::collections::{VecDeque, vec_deque};
use std::iter::IntoIterator;
use std::ops::{Deref, Range, RangeTo, RangeFrom, RangeFull, Index, IndexMut};

use index::{self, Point, Line, Column, IndexRange, RangeInclusive};

mod row;
pub use self::row::Row;

#[cfg(test)]
mod tests;

/// Convert a type to a linear index range.
pub trait ToRange {
    fn to_range(&self) -> RangeInclusive<index::Linear>;
}

/// Bidirection iterator
pub trait BidirectionalIterator: Iterator {
    fn prev(&mut self) -> Option<Self::Item>;
}

/// An item in the grid along with its Line and Column.
pub struct Indexed<T> {
    pub inner: T,
    pub line: Line,
    pub column: Column,
}

impl<T> Deref for Indexed<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.inner
    }
}

/// Represents the terminal display contents
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Grid<T> {
    /// Lines in the grid. Each row holds a list of cells corresponding to the
    /// columns in that row.
    raw: VecDeque<Row<T>>,

    /// Number of columns
    cols: index::Column,

    /// Number of lines.
    ///
    /// Invariant: lines is equivalent to raw.len()
    lines: index::Line,
}

pub struct GridIterator<'a, T: 'a> {
    grid: &'a Grid<T>,
    pub cur: Point,
}

impl<T: Clone> Grid<T> {
    pub fn new(lines: index::Line, cols: index::Column, template: &T) -> Grid<T> {
        let mut raw = VecDeque::with_capacity(*lines);
        for _ in IndexRange(index::Line(0)..lines) {
            raw.push_back(Row::new(cols, template));
        }

        Grid {
            raw: raw,
            cols: cols,
            lines: lines,
        }
    }

    pub fn resize(&mut self, lines: index::Line, cols: index::Column, template: &T) {
        // Check that there's actually work to do and return early if not
        if lines == self.lines && cols == self.cols {
            return;
        }

        match self.lines.cmp(&lines) {
            Ordering::Less => self.grow_lines(lines, template),
            Ordering::Greater => self.shrink_lines(lines),
            Ordering::Equal => (),
        }

        match self.cols.cmp(&cols) {
            Ordering::Less => self.grow_cols(cols, template),
            Ordering::Greater => self.shrink_cols(cols),
            Ordering::Equal => (),
        }
    }

    fn grow_lines(&mut self, lines: index::Line, template: &T) {
        for _ in IndexRange(self.num_lines()..lines) {
            self.raw.push_back(Row::new(self.cols, template));
        }

        self.lines = lines;
    }

    fn grow_cols(&mut self, cols: index::Column, template: &T) {
        for row in self.raw.iter_mut() {
            row.grow(cols, template);
        }

        self.cols = cols;
    }

}

impl<T> Grid<T> {
    #[inline]
    pub fn num_lines(&self) -> index::Line {
        self.lines
    }

    #[inline]
    pub fn num_cols(&self) -> index::Column {
        self.cols
    }

    #[inline]
    pub fn scroll_down(&mut self, region: &Range<index::Line>, positions: index::Line) {
        if region.start == Line(0) && region.end == self.num_lines() {
            // Full rotation
            for _ in 0..positions.0 {
                let item = self.raw.pop_back().unwrap();
                self.raw.push_front(item);
            }
        } else {
            // Subregion rotation
            for line in IndexRange((region.start + positions)..region.end).rev() {
                self.swap_lines(line, line - positions);
            }
        }
    }

    #[inline]
    pub fn scroll_up(&mut self, region: &Range<index::Line>, positions: index::Line) {
        if region.start == Line(0) && region.end == self.num_lines() {
            // Full rotation
            for _ in 0..positions.0 {
                let item = self.raw.pop_front().unwrap();
                self.raw.push_back(item);
            }
        } else {
            // Subregion rotation
            for line in IndexRange(region.start..(region.end - positions)) {
                self.swap_lines(line, line + positions);
            }
        }
    }

    pub fn iter_from(&self, point: Point) -> GridIterator<T> {
        GridIterator {
            grid: self,
            cur: point,
        }
    }

    #[inline]
    pub fn contains(&self, point: &Point) -> bool {
        self.lines > point.line && self.cols > point.col
    }

    /// Swap two lines in the grid
    ///
    /// This could have used slice::swap internally, but we are able to have
    /// better error messages by doing the bounds checking ourselves.
    #[inline]
    pub fn swap_lines(&mut self, src: index::Line, dst: index::Line) {
        self.raw.swap(*src, *dst);
    }

    fn shrink_lines(&mut self, lines: index::Line) {
        while index::Line(self.raw.len()) != lines {
            self.raw.pop_back();
        }

        self.lines = lines;
    }

    fn shrink_cols(&mut self, cols: index::Column) {
        for row in self.raw.iter_mut() {
            row.shrink(cols);
        }

        self.cols = cols;
    }
}

impl<'a, T> Iterator for GridIterator<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let last_line = self.grid.num_lines() - Line(1);
        let last_col = self.grid.num_cols() - Column(1);
        match self.cur {
            Point { line, col } if
                (line == last_line) &&
                (col == last_col) => None,
            Point { col, .. } if
                (col == last_col) => {
                self.cur.line += Line(1);
                self.cur.col = Column(0);
                Some(&self.grid[self.cur.line][self.cur.col])
            },
            _ => {
                self.cur.col += Column(1);
                Some(&self.grid[self.cur.line][self.cur.col])
            }
        }
    }
}

impl<'a, T> BidirectionalIterator for GridIterator<'a, T> {
    fn prev(&mut self) -> Option<Self::Item> {
        let num_cols = self.grid.num_cols();

        match self.cur {
            Point { line: Line(0), col: Column(0) } => None,
            Point { col: Column(0), .. } => {
                self.cur.line -= Line(1);
                self.cur.col = num_cols - Column(1);
                Some(&self.grid[self.cur.line][self.cur.col])
            },
            _ => {
                self.cur.col -= Column(1);
                Some(&self.grid[self.cur.line][self.cur.col])
            }
        }
    }
}

impl<T> Index<index::Line> for Grid<T> {
    type Output = Row<T>;

    #[inline]
    fn index(&self, index: index::Line) -> &Row<T> {
        &self.raw[index.0]
    }
}

impl<T> IndexMut<index::Line> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, index: index::Line) -> &mut Row<T> {
        &mut self.raw[index.0]
    }
}

impl<'point, T> Index<&'point Point> for Grid<T> {
    type Output = T;

    #[inline]
    fn index<'a>(&'a self, point: &Point) -> &'a T {
        &self.raw[point.line.0][point.col]
    }
}

impl<'point, T> IndexMut<&'point Point> for Grid<T> {
    #[inline]
    fn index_mut<'a, 'b>(&'a mut self, point: &'b Point) -> &'a mut T {
        &mut self.raw[point.line.0][point.col]
    }
}

impl<'a, T> IntoIterator for &'a Grid<T> {
    type Item = &'a Row<T>;
    type IntoIter = vec_deque::Iter<'a, Row<T>>;

    #[inline]
    fn into_iter(self) -> vec_deque::Iter<'a, Row<T>> {
        self.raw.iter()
    }
}

// =================================================================================================
// Regions =========================================================================================
// =================================================================================================

/// A subset of lines in the grid
///
/// May be constructed using Grid::region(..)
pub struct Region<'a, T: 'a> {
    start: Line,
    end: Line,
    raw: &'a VecDeque<Row<T>>,
}

/// A mutable subset of lines in the grid
///
/// May be constructed using Grid::region_mut(..)
pub struct RegionMut<'a, T: 'a> {
    start: Line,
    end: Line,
    raw: &'a mut VecDeque<Row<T>>,
}

impl<'a, T> RegionMut<'a, T> {
    /// Call the provided function for every item in this region
    pub fn each<F: Fn(&mut T)>(self, func: F) {
        for row in self {
            for item in row {
                func(item)
            }
        }
    }
}

pub trait IndexRegion<I, T> {
    /// Get an immutable region of Self
    fn region<'a>(&'a self, _: I) -> Region<'a, T>;

    /// Get a mutable region of Self
    fn region_mut<'a>(&'a mut self, _: I) -> RegionMut<'a, T>;
}

impl<T> IndexRegion<Range<Line>, T> for Grid<T> {
    fn region(&self, index: Range<Line>) -> Region<T> {
        assert!(index.start < self.num_lines());
        assert!(index.end <= self.num_lines());
        assert!(index.start <= index.end);
        Region {
            start: index.start,
            end: index.end,
            raw: &self.raw
        }
    }
    fn region_mut(&mut self, index: Range<Line>) -> RegionMut<T> {
        assert!(index.start < self.num_lines());
        assert!(index.end <= self.num_lines());
        assert!(index.start <= index.end);
        RegionMut {
            start: index.start,
            end: index.end,
            raw: &mut self.raw
        }
    }
}

impl<T> IndexRegion<RangeTo<Line>, T> for Grid<T> {
    fn region(&self, index: RangeTo<Line>) -> Region<T> {
        assert!(index.end <= self.num_lines());
        Region {
            start: Line(0),
            end: index.end,
            raw: &self.raw
        }
    }
    fn region_mut(&mut self, index: RangeTo<Line>) -> RegionMut<T> {
        assert!(index.end <= self.num_lines());
        RegionMut {
            start: Line(0),
            end: index.end,
            raw: &mut self.raw
        }
    }
}

impl<T> IndexRegion<RangeFrom<Line>, T> for Grid<T> {
    fn region(&self, index: RangeFrom<Line>) -> Region<T> {
        assert!(index.start < self.num_lines());
        Region {
            start: index.start,
            end: self.num_lines(),
            raw: &self.raw
        }
    }
    fn region_mut(&mut self, index: RangeFrom<Line>) -> RegionMut<T> {
        assert!(index.start < self.num_lines());
        RegionMut {
            start: index.start,
            end: self.num_lines(),
            raw: &mut self.raw
        }
    }
}

impl<T> IndexRegion<RangeFull, T> for Grid<T> {
    fn region(&self, _: RangeFull) -> Region<T> {
        Region {
            start: Line(0),
            end: self.num_lines(),
            raw: &self.raw
        }
    }

    fn region_mut(&mut self, _: RangeFull) -> RegionMut<T> {
        RegionMut {
            start: Line(0),
            end: self.num_lines(),
            raw: &mut self.raw
        }
    }
}

pub struct RegionIter<'a, T: 'a> {
    end: Line,
    cur: Line,
    raw: &'a VecDeque<Row<T>>,
}

pub struct RegionIterMut<'a, T: 'a> {
    end: Line,
    cur: Line,
    raw: &'a mut VecDeque<Row<T>>,
}

impl<'a, T> IntoIterator for Region<'a, T> {
    type Item = &'a Row<T>;
    type IntoIter = RegionIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        RegionIter {
            end: self.end,
            cur: self.start,
            raw: self.raw
        }
    }
}

impl<'a, T> IntoIterator for RegionMut<'a, T> {
    type Item = &'a mut Row<T>;
    type IntoIter = RegionIterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        RegionIterMut {
            end: self.end,
            cur: self.start,
            raw: self.raw
        }
    }
}

impl<'a, T> Iterator for RegionIter<'a, T> {
    type Item = &'a Row<T>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.cur < self.end {
            let index = self.cur;
            self.cur += 1;
            Some(&self.raw[*index])
        } else {
            None
        }
    }
}

impl<'a, T> Iterator for RegionIterMut<'a, T> {
    type Item = &'a mut Row<T>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.cur < self.end {
            let index = self.cur;
            self.cur += 1;
            unsafe {
                Some(&mut *(&mut self.raw[index.0] as *mut _))
            }
        } else {
            None
        }
    }
}
