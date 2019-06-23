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

//! A specialized 2d grid implementation optimized for use in a terminal.

use std::cmp::{max, min, Ordering};
use std::ops::{Deref, Index, IndexMut, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo};

use crate::index::{self, Column, IndexRange, Line, Point};
use crate::selection::Selection;

mod row;
pub use self::row::Row;

#[cfg(test)]
mod tests;

mod storage;
use self::storage::Storage;

const MIN_INIT_SIZE: usize = 1_000;

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

impl<T: PartialEq> ::std::cmp::PartialEq for Grid<T> {
    fn eq(&self, other: &Self) -> bool {
        // Compare struct fields and check result of grid comparison
        self.raw.eq(&other.raw)
            && self.cols.eq(&other.cols)
            && self.lines.eq(&other.lines)
            && self.display_offset.eq(&other.display_offset)
            && self.scroll_limit.eq(&other.scroll_limit)
            && self.selection.eq(&other.selection)
            && self.url_highlight.eq(&other.url_highlight)
    }
}

pub trait GridCell {
    fn is_empty(&self) -> bool;
    fn is_wrap(&self) -> bool;
    fn set_wrap(&mut self, wrap: bool);
}

/// Represents the terminal display contents
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Grid<T> {
    /// Lines in the grid. Each row holds a list of cells corresponding to the
    /// columns in that row.
    raw: Storage<T>,

    /// Number of columns
    cols: index::Column,

    /// Number of lines.
    ///
    /// Invariant: lines is equivalent to raw.len()
    lines: index::Line,

    /// Offset of displayed area
    ///
    /// If the displayed region isn't at the bottom of the screen, it stays
    /// stationary while more text is emitted. The scrolling implementation
    /// updates this offset accordingly.
    #[serde(default)]
    display_offset: usize,

    /// An limit on how far back it's possible to scroll
    #[serde(default)]
    scroll_limit: usize,

    /// Selected region
    #[serde(skip)]
    pub selection: Option<Selection>,

    #[serde(default)]
    max_scroll_limit: usize,

    /// Range for URL hover highlights
    #[serde(default)]
    pub url_highlight: Option<RangeInclusive<index::Linear>>,
}

#[derive(Copy, Clone)]
pub enum Scroll {
    Lines(isize),
    PageUp,
    PageDown,
    Top,
    Bottom,
}

#[derive(Copy, Clone)]
pub enum ViewportPosition {
    Visible(Line),
    Above,
    Below,
}

impl<T: GridCell + Copy + Clone> Grid<T> {
    pub fn new(lines: index::Line, cols: index::Column, scrollback: usize, template: T) -> Grid<T> {
        let raw = Storage::with_capacity(lines, Row::new(cols, &template));
        Grid {
            raw,
            cols,
            lines,
            display_offset: 0,
            scroll_limit: 0,
            selection: None,
            max_scroll_limit: scrollback,
            url_highlight: None,
        }
    }

    pub fn visible_to_buffer(&self, point: Point) -> Point<usize> {
        Point { line: self.visible_line_to_buffer(point.line), col: point.col }
    }

    pub fn buffer_line_to_visible(&self, line: usize) -> ViewportPosition {
        let offset = line.saturating_sub(self.display_offset);
        if line < self.display_offset {
            ViewportPosition::Below
        } else if offset >= *self.num_lines() {
            ViewportPosition::Above
        } else {
            ViewportPosition::Visible(self.lines - offset - 1)
        }
    }

    pub fn visible_line_to_buffer(&self, line: Line) -> usize {
        self.line_to_offset(line) + self.display_offset
    }

    /// Update the size of the scrollback history
    pub fn update_history(&mut self, history_size: usize, template: &T) {
        self.raw.update_history(history_size, Row::new(self.cols, &template));
        self.max_scroll_limit = history_size;
        self.scroll_limit = min(self.scroll_limit, history_size);
        self.display_offset = min(self.display_offset, self.scroll_limit);
    }

    pub fn scroll_display(&mut self, scroll: Scroll) {
        match scroll {
            Scroll::Lines(count) => {
                self.display_offset = min(
                    max((self.display_offset as isize) + count, 0isize) as usize,
                    self.scroll_limit,
                );
            },
            Scroll::PageUp => {
                self.display_offset = min(self.display_offset + self.lines.0, self.scroll_limit);
            },
            Scroll::PageDown => {
                self.display_offset -= min(self.display_offset, self.lines.0);
            },
            Scroll::Top => self.display_offset = self.scroll_limit,
            Scroll::Bottom => self.display_offset = 0,
        }
    }

    pub fn resize(
        &mut self,
        reflow: bool,
        lines: index::Line,
        cols: index::Column,
        cursor_pos: &mut Point,
        template: &T,
    ) {
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
            Ordering::Less => self.grow_cols(reflow, cols, cursor_pos, template),
            Ordering::Greater => self.shrink_cols(reflow, cols, template),
            Ordering::Equal => (),
        }
    }

    fn increase_scroll_limit(&mut self, count: usize, template: &T) {
        self.scroll_limit = min(self.scroll_limit + count, self.max_scroll_limit);

        // Initialize new lines when the history buffer is smaller than the scroll limit
        let history_size = self.raw.len().saturating_sub(*self.lines);
        if history_size < self.scroll_limit {
            let new = min(
                max(self.scroll_limit - history_size, MIN_INIT_SIZE),
                self.max_scroll_limit - history_size,
            );
            self.raw.initialize(new, Row::new(self.cols, template));
        }
    }

    fn decrease_scroll_limit(&mut self, count: usize) {
        self.scroll_limit = self.scroll_limit.saturating_sub(count);
    }

    /// Add lines to the visible area
    ///
    /// Alacritty keeps the cursor at the bottom of the terminal as long as there
    /// is scrollback available. Once scrollback is exhausted, new lines are
    /// simply added to the bottom of the screen.
    fn grow_lines(&mut self, new_line_count: index::Line, template: &T) {
        let lines_added = new_line_count - self.lines;

        // Need to "resize" before updating buffer
        self.raw.grow_visible_lines(new_line_count, Row::new(self.cols, template));
        self.lines = new_line_count;

        // Move existing lines up if there is no scrollback to fill new lines
        if lines_added.0 > self.scroll_limit {
            let scroll_lines = lines_added - self.scroll_limit;
            self.scroll_up(&(Line(0)..new_line_count), scroll_lines, template);
        }

        self.scroll_limit = self.scroll_limit.saturating_sub(*lines_added);
        self.display_offset = self.display_offset.saturating_sub(*lines_added);
    }

    fn grow_cols(
        &mut self,
        reflow: bool,
        cols: index::Column,
        cursor_pos: &mut Point,
        template: &T,
    ) {
        let mut new_empty_lines = 0;
        let mut new_raw: Vec<Row<T>> = Vec::with_capacity(self.raw.len());
        for (i, mut row) in self.raw.drain().enumerate().rev() {
            if let Some(last_row) = new_raw.last_mut() {
                // Grow the current line if there's wrapped content available
                if reflow
                    && last_row.len() < cols.0
                    && last_row.last().map(GridCell::is_wrap) == Some(true)
                {
                    // Remove wrap flag before appending additional cells
                    if let Some(cell) = last_row.last_mut() {
                        cell.set_wrap(false);
                    }

                    // Append as many cells from the next line as possible
                    let len = min(row.len(), cols.0 - last_row.len());
                    let mut cells = row.front_split_off(len);
                    last_row.append(&mut cells);

                    if row.is_empty() {
                        let raw_len = i + 1 + new_raw.len();;
                        if raw_len < self.lines.0 || self.scroll_limit == 0 {
                            // Add new line and move lines up if we can't pull from history
                            cursor_pos.line = Line(cursor_pos.line.saturating_sub(1));
                            new_empty_lines += 1;
                        } else {
                            // Make sure viewport doesn't move if line is outside of the visible
                            // area
                            if i < self.display_offset {
                                self.display_offset = self.display_offset.saturating_sub(1);
                            }

                            // Remove one line from scrollback, since we just moved it to the
                            // viewport
                            self.scroll_limit = self.scroll_limit.saturating_sub(1);
                            self.display_offset = min(self.display_offset, self.scroll_limit);
                        }

                        // Don't push line into the new buffer
                        continue;
                    } else if let Some(cell) = last_row.last_mut() {
                        // Set wrap flag if next line still has cells
                        cell.set_wrap(true);
                    }
                }
            }

            new_raw.push(row);
        }

        // Add padding lines
        new_raw.append(&mut vec![Row::new(cols, template); new_empty_lines]);

        // Fill remaining cells and reverse iterator
        let mut reversed = Vec::with_capacity(new_raw.len());
        for mut row in new_raw.drain(..).rev() {
            if row.len() < cols.0 {
                row.grow(cols, template);
            }
            reversed.push(row);
        }

        self.raw.replace_inner(reversed);

        self.cols = cols;
    }

    fn shrink_cols(&mut self, reflow: bool, cols: index::Column, template: &T) {
        let mut new_raw = Vec::with_capacity(self.raw.len());
        let mut buffered = None;
        for (i, mut row) in self.raw.drain().enumerate().rev() {
            if let Some(buffered) = buffered.take() {
                row.append_front(buffered);
            }

            let mut wrapped = row.shrink(cols);
            new_raw.push(row);

            while let (Some(mut wrapped_cells), true) = (wrapped.take(), reflow) {
                // Set line as wrapped if cells got removed
                if let Some(cell) = new_raw.last_mut().and_then(|r| r.last_mut()) {
                    cell.set_wrap(true);
                }

                if Some(true) == wrapped_cells.last().map(|c| c.is_wrap() && i >= 1)
                    && wrapped_cells.len() < cols.0
                {
                    // Make sure previous wrap flag doesn't linger around
                    if let Some(cell) = wrapped_cells.last_mut() {
                        cell.set_wrap(false);
                    }

                    // Add removed cells to start of next row
                    buffered = Some(wrapped_cells);
                } else {
                    // Make sure viewport doesn't move if line is outside of the visible area
                    if i < self.display_offset {
                        self.display_offset = min(self.display_offset + 1, self.max_scroll_limit);
                    }

                    // Make sure new row is at least as long as new width
                    let occ = wrapped_cells.len();
                    if occ < cols.0 {
                        wrapped_cells.append(&mut vec![*template; cols.0 - occ]);
                    }
                    let mut row = Row::from_vec(wrapped_cells, occ);

                    // Since inserted might exceed cols, we need to check it again
                    wrapped = row.shrink(cols);

                    // Add new row with all removed cells
                    new_raw.push(row);

                    // Increase scrollback history
                    self.scroll_limit = min(self.scroll_limit + 1, self.max_scroll_limit);
                }
            }
        }

        let mut reversed: Vec<Row<T>> = new_raw.drain(..).rev().collect();
        reversed.truncate(self.max_scroll_limit + self.lines.0);
        self.raw.replace_inner(reversed);
        self.cols = cols;
    }

    /// Remove lines from the visible area
    ///
    /// The behavior in Terminal.app and iTerm.app is to keep the cursor at the
    /// bottom of the screen. This is achieved by pushing history "out the top"
    /// of the terminal window.
    ///
    /// Alacritty takes the same approach.
    fn shrink_lines(&mut self, target: index::Line) {
        let prev = self.lines;

        self.selection = None;
        self.url_highlight = None;
        self.raw.rotate(*prev as isize - *target as isize);
        self.raw.shrink_visible_lines(target);
        self.lines = target;
    }

    /// Convert a Line index (active region) to a buffer offset
    ///
    /// # Panics
    ///
    /// This method will panic if `Line` is larger than the grid dimensions
    pub fn line_to_offset(&self, line: index::Line) -> usize {
        assert!(line < self.num_lines());

        *(self.num_lines() - line - 1)
    }

    #[inline]
    pub fn scroll_down(
        &mut self,
        region: &Range<index::Line>,
        positions: index::Line,
        template: &T,
    ) {
        // Whether or not there is a scrolling region active, as long as it
        // starts at the top, we can do a full rotation which just involves
        // changing the start index.
        //
        // To accommodate scroll regions, rows are reordered at the end.
        if region.start == Line(0) {
            // Rotate the entire line buffer. If there's a scrolling region
            // active, the bottom lines are restored in the next step.
            self.raw.rotate_up(*positions);
            if let Some(ref mut selection) = self.selection {
                selection.rotate(-(*positions as isize));
            }
            self.url_highlight = None;

            self.decrease_scroll_limit(*positions);

            // Now, restore any scroll region lines
            let lines = self.lines;
            for i in IndexRange(region.end..lines) {
                self.raw.swap_lines(i, i + positions);
            }

            // Finally, reset recycled lines
            for i in IndexRange(Line(0)..positions) {
                self.raw[i].reset(&template);
            }
        } else {
            // Subregion rotation
            for line in IndexRange((region.start + positions)..region.end).rev() {
                self.raw.swap_lines(line, line - positions);
            }

            for line in IndexRange(region.start..(region.start + positions)) {
                self.raw[line].reset(&template);
            }
        }
    }

    /// scroll_up moves lines at the bottom towards the top
    ///
    /// This is the performance-sensitive part of scrolling.
    pub fn scroll_up(&mut self, region: &Range<index::Line>, positions: index::Line, template: &T) {
        if region.start == Line(0) {
            // Update display offset when not pinned to active area
            if self.display_offset != 0 {
                self.display_offset =
                    min(self.display_offset + *positions, self.len() - self.num_lines().0);
            }

            self.increase_scroll_limit(*positions, template);

            // Rotate the entire line buffer. If there's a scrolling region
            // active, the bottom lines are restored in the next step.
            self.raw.rotate(-(*positions as isize));
            if let Some(ref mut selection) = self.selection {
                selection.rotate(*positions as isize);
            }
            self.url_highlight = None;

            // // This next loop swaps "fixed" lines outside of a scroll region
            // // back into place after the rotation. The work is done in buffer-
            // // space rather than terminal-space to avoid redundant
            // // transformations.
            let fixed_lines = *self.num_lines() - *region.end;

            for i in 0..fixed_lines {
                self.raw.swap(i, i + *positions);
            }

            // Finally, reset recycled lines
            //
            // Recycled lines are just above the end of the scrolling region.
            for i in 0..*positions {
                self.raw[i + fixed_lines].reset(&template);
            }
        } else {
            // Subregion rotation
            for line in IndexRange(region.start..(region.end - positions)) {
                self.raw.swap_lines(line, line + positions);
            }

            // Clear reused lines
            for line in IndexRange((region.end - positions)..region.end) {
                self.raw[line].reset(&template);
            }
        }
    }

    // Completely reset the grid state
    pub fn reset(&mut self, template: &T) {
        // Explicitly purge all lines from history
        let shrinkage = self.raw.len() - self.lines.0;
        self.raw.shrink_lines(shrinkage);
        self.clear_history();

        // Reset all visible lines
        for row in 0..self.raw.len() {
            self.raw[row].reset(template);
        }

        self.display_offset = 0;
        self.selection = None;
        self.url_highlight = None;
    }
}

#[allow(clippy::len_without_is_empty)]
impl<T> Grid<T> {
    #[inline]
    pub fn num_lines(&self) -> index::Line {
        self.lines
    }

    pub fn display_iter(&self) -> DisplayIter<'_, T> {
        DisplayIter::new(self)
    }

    #[inline]
    pub fn num_cols(&self) -> index::Column {
        self.cols
    }

    pub fn clear_history(&mut self) {
        self.scroll_limit = 0;
    }

    #[inline]
    pub fn scroll_limit(&self) -> usize {
        self.scroll_limit
    }

    /// Total number of lines in the buffer, this includes scrollback + visible lines
    #[inline]
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    #[inline]
    pub fn history_size(&self) -> usize {
        self.raw.len().saturating_sub(*self.lines)
    }

    /// This is used only for initializing after loading ref-tests
    pub fn initialize_all(&mut self, template: &T)
    where
        T: Copy,
    {
        let history_size = self.raw.len().saturating_sub(*self.lines);
        self.raw.initialize(self.max_scroll_limit - history_size, Row::new(self.cols, template));
    }

    /// This is used only for truncating before saving ref-tests
    pub fn truncate(&mut self) {
        self.raw.truncate();
    }

    pub fn iter_from(&self, point: Point<usize>) -> GridIterator<'_, T> {
        GridIterator { grid: self, cur: point }
    }

    #[inline]
    pub fn contains(&self, point: &Point) -> bool {
        self.lines > point.line && self.cols > point.col
    }

    #[inline]
    pub fn display_offset(&self) -> usize {
        self.display_offset
    }
}

pub struct GridIterator<'a, T> {
    /// Immutable grid reference
    grid: &'a Grid<T>,

    /// Current position of the iterator within the grid.
    pub cur: Point<usize>,
}

impl<'a, T> Iterator for GridIterator<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        let last_col = self.grid.num_cols() - Column(1);
        match self.cur {
            Point { line, col } if line == 0 && col == last_col => None,
            Point { col, .. } if (col == last_col) => {
                self.cur.line -= 1;
                self.cur.col = Column(0);
                Some(&self.grid[self.cur.line][self.cur.col])
            },
            _ => {
                self.cur.col += Column(1);
                Some(&self.grid[self.cur.line][self.cur.col])
            },
        }
    }
}

impl<'a, T> BidirectionalIterator for GridIterator<'a, T> {
    fn prev(&mut self) -> Option<Self::Item> {
        let num_cols = self.grid.num_cols();

        match self.cur {
            Point { line, col: Column(0) } if line == self.grid.len() - 1 => None,
            Point { col: Column(0), .. } => {
                self.cur.line += 1;
                self.cur.col = num_cols - Column(1);
                Some(&self.grid[self.cur.line][self.cur.col])
            },
            _ => {
                self.cur.col -= Column(1);
                Some(&self.grid[self.cur.line][self.cur.col])
            },
        }
    }
}

/// Index active region by line
impl<T> Index<index::Line> for Grid<T> {
    type Output = Row<T>;

    #[inline]
    fn index(&self, index: index::Line) -> &Row<T> {
        &self.raw[index]
    }
}

/// Index with buffer offset
impl<T> Index<usize> for Grid<T> {
    type Output = Row<T>;

    #[inline]
    fn index(&self, index: usize) -> &Row<T> {
        &self.raw[index]
    }
}

impl<T> IndexMut<index::Line> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, index: index::Line) -> &mut Row<T> {
        &mut self.raw[index]
    }
}

impl<T> IndexMut<usize> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Row<T> {
        &mut self.raw[index]
    }
}

impl<'point, T> Index<&'point Point> for Grid<T> {
    type Output = T;

    #[inline]
    fn index<'a>(&'a self, point: &Point) -> &'a T {
        &self[point.line][point.col]
    }
}

impl<'point, T> IndexMut<&'point Point> for Grid<T> {
    #[inline]
    fn index_mut<'a, 'b>(&'a mut self, point: &'b Point) -> &'a mut T {
        &mut self[point.line][point.col]
    }
}

// -------------------------------------------------------------------------------------------------
// REGIONS
// -------------------------------------------------------------------------------------------------

/// A subset of lines in the grid
///
/// May be constructed using Grid::region(..)
pub struct Region<'a, T> {
    start: Line,
    end: Line,
    raw: &'a Storage<T>,
}

/// A mutable subset of lines in the grid
///
/// May be constructed using Grid::region_mut(..)
pub struct RegionMut<'a, T> {
    start: Line,
    end: Line,
    raw: &'a mut Storage<T>,
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
    fn region(&self, _: I) -> Region<'_, T>;

    /// Get a mutable region of Self
    fn region_mut(&mut self, _: I) -> RegionMut<'_, T>;
}

impl<T> IndexRegion<Range<Line>, T> for Grid<T> {
    fn region(&self, index: Range<Line>) -> Region<'_, T> {
        assert!(index.start < self.num_lines());
        assert!(index.end <= self.num_lines());
        assert!(index.start <= index.end);
        Region { start: index.start, end: index.end, raw: &self.raw }
    }

    fn region_mut(&mut self, index: Range<Line>) -> RegionMut<'_, T> {
        assert!(index.start < self.num_lines());
        assert!(index.end <= self.num_lines());
        assert!(index.start <= index.end);
        RegionMut { start: index.start, end: index.end, raw: &mut self.raw }
    }
}

impl<T> IndexRegion<RangeTo<Line>, T> for Grid<T> {
    fn region(&self, index: RangeTo<Line>) -> Region<'_, T> {
        assert!(index.end <= self.num_lines());
        Region { start: Line(0), end: index.end, raw: &self.raw }
    }

    fn region_mut(&mut self, index: RangeTo<Line>) -> RegionMut<'_, T> {
        assert!(index.end <= self.num_lines());
        RegionMut { start: Line(0), end: index.end, raw: &mut self.raw }
    }
}

impl<T> IndexRegion<RangeFrom<Line>, T> for Grid<T> {
    fn region(&self, index: RangeFrom<Line>) -> Region<'_, T> {
        assert!(index.start < self.num_lines());
        Region { start: index.start, end: self.num_lines(), raw: &self.raw }
    }

    fn region_mut(&mut self, index: RangeFrom<Line>) -> RegionMut<'_, T> {
        assert!(index.start < self.num_lines());
        RegionMut { start: index.start, end: self.num_lines(), raw: &mut self.raw }
    }
}

impl<T> IndexRegion<RangeFull, T> for Grid<T> {
    fn region(&self, _: RangeFull) -> Region<'_, T> {
        Region { start: Line(0), end: self.num_lines(), raw: &self.raw }
    }

    fn region_mut(&mut self, _: RangeFull) -> RegionMut<'_, T> {
        RegionMut { start: Line(0), end: self.num_lines(), raw: &mut self.raw }
    }
}

pub struct RegionIter<'a, T> {
    end: Line,
    cur: Line,
    raw: &'a Storage<T>,
}

pub struct RegionIterMut<'a, T> {
    end: Line,
    cur: Line,
    raw: &'a mut Storage<T>,
}

impl<'a, T> IntoIterator for Region<'a, T> {
    type IntoIter = RegionIter<'a, T>;
    type Item = &'a Row<T>;

    fn into_iter(self) -> Self::IntoIter {
        RegionIter { end: self.end, cur: self.start, raw: self.raw }
    }
}

impl<'a, T> IntoIterator for RegionMut<'a, T> {
    type IntoIter = RegionIterMut<'a, T>;
    type Item = &'a mut Row<T>;

    fn into_iter(self) -> Self::IntoIter {
        RegionIterMut { end: self.end, cur: self.start, raw: self.raw }
    }
}

impl<'a, T> Iterator for RegionIter<'a, T> {
    type Item = &'a Row<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cur < self.end {
            let index = self.cur;
            self.cur += 1;
            Some(&self.raw[index])
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
            unsafe { Some(&mut *(&mut self.raw[index] as *mut _)) }
        } else {
            None
        }
    }
}

// -------------------------------------------------------------------------------------------------
// DISPLAY ITERATOR
// -------------------------------------------------------------------------------------------------

/// Iterates over the visible area accounting for buffer transform
pub struct DisplayIter<'a, T> {
    grid: &'a Grid<T>,
    offset: usize,
    limit: usize,
    col: Column,
    line: Line,
}

impl<'a, T: 'a> DisplayIter<'a, T> {
    pub fn new(grid: &'a Grid<T>) -> DisplayIter<'a, T> {
        let offset = grid.display_offset + *grid.num_lines() - 1;
        let limit = grid.display_offset;
        let col = Column(0);
        let line = Line(0);

        DisplayIter { grid, offset, col, limit, line }
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn column(&self) -> Column {
        self.col
    }

    pub fn line(&self) -> Line {
        self.line
    }
}

impl<'a, T: Copy + 'a> Iterator for DisplayIter<'a, T> {
    type Item = Indexed<T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Return None if we've reached the end.
        if self.offset == self.limit && self.grid.num_cols() == self.col {
            return None;
        }

        // Get the next item.
        let item = Some(Indexed {
            inner: self.grid.raw[self.offset][self.col],
            line: self.line,
            column: self.col,
        });

        // Update line/col to point to next item
        self.col += 1;
        if self.col == self.grid.num_cols() && self.offset != self.limit {
            self.offset -= 1;

            self.col = Column(0);
            self.line = Line(*self.grid.lines - 1 - (self.offset - self.limit));
        }

        item
    }
}
