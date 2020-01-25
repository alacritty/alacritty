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
use std::ops::{Deref, Index, IndexMut, Range, RangeFrom, RangeFull, RangeTo};

use serde::{Deserialize, Serialize};

use crate::index::{Column, IndexRange, Line, Point};
use crate::selection::Selection;
use crate::term::cell::Flags;

mod row;
pub use self::row::Row;

#[cfg(test)]
mod tests;

mod storage;
use self::storage::Storage;

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
            && self.history_size().eq(&other.history_size())
            && self.selection.eq(&other.selection)
    }
}

pub trait GridCell {
    fn is_empty(&self) -> bool;
    fn flags(&self) -> &Flags;
    fn flags_mut(&mut self) -> &mut Flags;

    /// Fast equality approximation.
    ///
    /// This is a faster alternative to [`PartialEq`],
    /// but might report inequal cells as equal.
    fn fast_eq(&self, other: Self) -> bool;
}

/// Represents the terminal display contents
///
/// ```notrust
/// ┌─────────────────────────┐  <-- max_scroll_limit + lines
/// │                         │
/// │      UNINITIALIZED      │
/// │                         │
/// ├─────────────────────────┤  <-- raw.len()
/// │                         │
/// │      RESIZE BUFFER      │
/// │                         │
/// ├─────────────────────────┤  <-- scroll_limit + lines
/// │                         │
/// │     SCROLLUP REGION     │
/// │                         │
/// ├─────────────────────────┤v lines
/// │                         │|
/// │     VISIBLE  REGION     │|
/// │                         │|
/// ├─────────────────────────┤^ <-- display_offset
/// │                         │
/// │    SCROLLDOWN REGION    │
/// │                         │
/// └─────────────────────────┘  <-- zero
///                           ^
///                          cols
/// ```
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Grid<T> {
    /// Lines in the grid. Each row holds a list of cells corresponding to the
    /// columns in that row.
    raw: Storage<T>,

    /// Number of columns
    cols: Column,

    /// Number of visible lines.
    lines: Line,

    /// Offset of displayed area
    ///
    /// If the displayed region isn't at the bottom of the screen, it stays
    /// stationary while more text is emitted. The scrolling implementation
    /// updates this offset accordingly.
    display_offset: usize,

    /// Legacy value, however we can't remove it, since it's in all ref-tests
    #[serde(skip)]
    scroll_limit: usize,

    /// Selected region
    #[serde(skip)]
    pub selection: Option<Selection>,

    max_scroll_limit: usize,
}

#[derive(Copy, Clone)]
pub enum Scroll {
    Lines(isize),
    PageUp,
    PageDown,
    Top,
    Bottom,
}

impl<T: GridCell + PartialEq + Copy> Grid<T> {
    pub fn new(lines: Line, cols: Column, scrollback: usize, template: T) -> Grid<T> {
        let raw = Storage::with_capacity(lines, Row::new(cols, &template));
        Grid {
            raw,
            cols,
            lines,
            display_offset: 0,
            scroll_limit: 0,
            selection: None,
            max_scroll_limit: scrollback,
        }
    }

    pub fn buffer_to_visible(&self, point: impl Into<Point<usize>>) -> Option<Point<usize>> {
        let mut point = point.into();

        if point.line < self.display_offset || point.line >= self.display_offset + self.lines.0 {
            return None;
        }

        point.line = self.lines.0 + self.display_offset - point.line - 1;

        Some(point)
    }

    pub fn visible_to_buffer(&self, point: Point) -> Point<usize> {
        Point { line: self.visible_line_to_buffer(point.line), col: point.col }
    }

    fn visible_line_to_buffer(&self, line: Line) -> usize {
        self.line_to_offset(line) + self.display_offset
    }

    /// Update the size of the scrollback history
    pub fn update_history(&mut self, history_size: usize, template: &T) {
        self.raw.update_history(history_size, Row::new(self.cols, &template));
        self.max_scroll_limit = history_size;
        if history_size < self.history_size() {
            self.decrease_scroll_limit(self.history_size() - history_size);
        }
        self.display_offset = min(self.display_offset, self.history_size());
    }

    pub fn scroll_display(&mut self, scroll: Scroll) {
        match scroll {
            Scroll::Lines(count) => {
                self.display_offset = min(
                    max((self.display_offset as isize) + count, 0isize) as usize,
                    self.history_size(),
                );
            },
            Scroll::PageUp => {
                self.display_offset = min(self.display_offset + self.lines.0, self.history_size());
            },
            Scroll::PageDown => {
                self.display_offset -= min(self.display_offset, self.lines.0);
            },
            Scroll::Top => self.display_offset = self.history_size(),
            Scroll::Bottom => self.display_offset = 0,
        }
    }

    pub fn resize(
        &mut self,
        reflow: bool,
        lines: Line,
        cols: Column,
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
        let new = min(count, self.max_scroll_limit - self.history_size());
        self.raw.initialize(new, Row::new(self.cols, template));
    }

    fn decrease_scroll_limit(&mut self, count: usize) {
        let history_size = self.history_size();
        let count = history_size - history_size.saturating_sub(count);
        self.raw.shrink_lines(count);
    }

    /// Add lines to the visible area
    ///
    /// Alacritty keeps the cursor at the bottom of the terminal as long as there
    /// is scrollback available. Once scrollback is exhausted, new lines are
    /// simply added to the bottom of the screen.
    fn grow_lines(&mut self, new_line_count: Line, template: &T) {
        let lines_added = new_line_count - self.lines;

        // Need to "resize" before updating buffer
        self.raw.grow_visible_lines(new_line_count, Row::new(self.cols, template));
        self.lines = new_line_count;

        // Move existing lines up if there is no scrollback to fill new lines
        if lines_added.0 > self.history_size() {
            let scroll_lines = lines_added - self.history_size();
            self.scroll_up(&(Line(0)..new_line_count), scroll_lines, template);
        }

        self.decrease_scroll_limit(*lines_added);
        self.display_offset = self.display_offset.saturating_sub(*lines_added);
    }

    // Grow number of columns in each row, reflowing if necessary
    fn grow_cols(&mut self, reflow: bool, cols: Column, cursor_pos: &mut Point, template: &T) {
        // Check if a row needs to be wrapped
        let should_reflow = |row: &Row<T>| -> bool {
            let len = Column(row.len());
            reflow && len < cols && row[len - 1].flags().contains(Flags::WRAPLINE)
        };

        let mut new_empty_lines = 0;
        let mut reversed: Vec<Row<T>> = Vec::with_capacity(self.raw.len());
        for (i, mut row) in self.raw.drain().enumerate().rev() {
            // FIXME: Rust 1.39.0+ allows moving in pattern guard here
            // Check if reflowing shoud be performed
            let mut last_row = reversed.last_mut();
            let last_row = match last_row {
                Some(ref mut last_row) if should_reflow(last_row) => last_row,
                _ => {
                    reversed.push(row);
                    continue;
                },
            };

            // Remove wrap flag before appending additional cells
            if let Some(cell) = last_row.last_mut() {
                cell.flags_mut().remove(Flags::WRAPLINE);
            }

            // Remove leading spacers when reflowing wide char to the previous line
            let last_len = last_row.len();
            if last_len >= 2
                && !last_row[Column(last_len - 2)].flags().contains(Flags::WIDE_CHAR)
                && last_row[Column(last_len - 1)].flags().contains(Flags::WIDE_CHAR_SPACER)
            {
                last_row.shrink(Column(last_len - 1));
            }

            // Append as many cells from the next line as possible
            let len = min(row.len(), cols.0 - last_row.len());

            // Insert leading spacer when there's not enough room for reflowing wide char
            let mut cells = if row[Column(len - 1)].flags().contains(Flags::WIDE_CHAR) {
                let mut cells = row.front_split_off(len - 1);

                let mut spacer = *template;
                spacer.flags_mut().insert(Flags::WIDE_CHAR_SPACER);
                cells.push(spacer);

                cells
            } else {
                row.front_split_off(len)
            };

            last_row.append(&mut cells);

            if row.is_empty() {
                if i + reversed.len() <= self.lines.0 {
                    // Add new line and move lines up if we can't pull from history
                    cursor_pos.line = Line(cursor_pos.line.saturating_sub(1));
                    new_empty_lines += 1;
                } else if i < self.display_offset {
                    // Keep viewport in place if line is outside of the visible area
                    self.display_offset = self.display_offset.saturating_sub(1);
                }

                // Don't push line into the new buffer
                continue;
            } else if let Some(cell) = last_row.last_mut() {
                // Set wrap flag if next line still has cells
                cell.flags_mut().insert(Flags::WRAPLINE);
            }

            reversed.push(row);
        }

        // Add padding lines
        reversed.append(&mut vec![Row::new(cols, template); new_empty_lines]);

        // Fill remaining cells and reverse iterator
        let mut new_raw = Vec::with_capacity(reversed.len());
        for mut row in reversed.drain(..).rev() {
            if row.len() < cols.0 {
                row.grow(cols, template);
            }
            new_raw.push(row);
        }

        self.raw.replace_inner(new_raw);

        self.display_offset = min(self.display_offset, self.history_size());
        self.cols = cols;
    }

    // Shrink number of columns in each row, reflowing if necessary
    fn shrink_cols(&mut self, reflow: bool, cols: Column, template: &T) {
        let mut new_raw = Vec::with_capacity(self.raw.len());
        let mut buffered = None;
        for (i, mut row) in self.raw.drain().enumerate().rev() {
            // Append lines left over from previous row
            if let Some(buffered) = buffered.take() {
                row.append_front(buffered);
            }

            loop {
                // FIXME: Rust 1.39.0+ allows moving in pattern guard here
                // Check if reflowing shoud be performed
                let wrapped = row.shrink(cols);
                let mut wrapped = match wrapped {
                    Some(_) if reflow => wrapped.unwrap(),
                    _ => {
                        new_raw.push(row);
                        break;
                    },
                };

                // Insert spacer if a wide char would be wrapped into the last column
                if row.len() >= cols.0 && row[cols - 1].flags().contains(Flags::WIDE_CHAR) {
                    wrapped.insert(0, row[cols - 1]);

                    let mut spacer = *template;
                    spacer.flags_mut().insert(Flags::WIDE_CHAR_SPACER);
                    row[cols - 1] = spacer;
                }

                // Remove wide char spacer before shrinking
                let len = wrapped.len();
                if (len == 1 || (len >= 2 && !wrapped[len - 2].flags().contains(Flags::WIDE_CHAR)))
                    && wrapped[len - 1].flags().contains(Flags::WIDE_CHAR_SPACER)
                {
                    if len == 1 {
                        row[cols - 1].flags_mut().insert(Flags::WRAPLINE);
                        new_raw.push(row);
                        break;
                    } else {
                        wrapped[len - 2].flags_mut().insert(Flags::WRAPLINE);
                        wrapped.truncate(len - 1);
                    }
                }

                new_raw.push(row);

                // Set line as wrapped if cells got removed
                if let Some(cell) = new_raw.last_mut().and_then(|r| r.last_mut()) {
                    cell.flags_mut().insert(Flags::WRAPLINE);
                }

                if wrapped
                    .last()
                    .map(|c| c.flags().contains(Flags::WRAPLINE) && i >= 1)
                    .unwrap_or(false)
                    && wrapped.len() < cols.0
                {
                    // Make sure previous wrap flag doesn't linger around
                    if let Some(cell) = wrapped.last_mut() {
                        cell.flags_mut().remove(Flags::WRAPLINE);
                    }

                    // Add removed cells to start of next row
                    buffered = Some(wrapped);
                    break;
                } else {
                    // Make sure viewport doesn't move if line is outside of the visible area
                    if i < self.display_offset {
                        self.display_offset = min(self.display_offset + 1, self.max_scroll_limit);
                    }

                    // Make sure new row is at least as long as new width
                    let occ = wrapped.len();
                    if occ < cols.0 {
                        wrapped.append(&mut vec![*template; cols.0 - occ]);
                    }
                    row = Row::from_vec(wrapped, occ);
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
    fn shrink_lines(&mut self, target: Line) {
        let prev = self.lines;

        self.selection = None;
        self.raw.rotate(*prev as isize - *target as isize);
        self.raw.shrink_visible_lines(target);
        self.lines = target;
    }

    /// Convert a Line index (active region) to a buffer offset
    ///
    /// # Panics
    ///
    /// This method will panic if `Line` is larger than the grid dimensions
    pub fn line_to_offset(&self, line: Line) -> usize {
        assert!(line < self.num_lines());

        *(self.num_lines() - line - 1)
    }

    #[inline]
    pub fn scroll_down(&mut self, region: &Range<Line>, positions: Line, template: &T) {
        let num_lines = self.num_lines().0;
        let num_cols = self.num_cols().0;

        // Whether or not there is a scrolling region active, as long as it
        // starts at the top, we can do a full rotation which just involves
        // changing the start index.
        //
        // To accommodate scroll regions, rows are reordered at the end.
        if region.start == Line(0) {
            // Rotate the entire line buffer. If there's a scrolling region
            // active, the bottom lines are restored in the next step.
            self.raw.rotate_up(*positions);
            self.selection = self
                .selection
                .take()
                .and_then(|s| s.rotate(num_lines, num_cols, region, -(*positions as isize)));

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
            // Rotate selection to track content
            self.selection = self
                .selection
                .take()
                .and_then(|s| s.rotate(num_lines, num_cols, region, -(*positions as isize)));

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
    pub fn scroll_up(&mut self, region: &Range<Line>, positions: Line, template: &T) {
        let num_lines = self.num_lines().0;
        let num_cols = self.num_cols().0;

        if region.start == Line(0) {
            // Update display offset when not pinned to active area
            if self.display_offset != 0 {
                self.display_offset = min(self.display_offset + *positions, self.len() - num_lines);
            }

            self.increase_scroll_limit(*positions, template);

            // Rotate the entire line buffer. If there's a scrolling region
            // active, the bottom lines are restored in the next step.
            self.raw.rotate(-(*positions as isize));
            self.selection = self
                .selection
                .take()
                .and_then(|s| s.rotate(num_lines, num_cols, region, *positions as isize));

            // This next loop swaps "fixed" lines outside of a scroll region
            // back into place after the rotation. The work is done in buffer-
            // space rather than terminal-space to avoid redundant
            // transformations.
            let fixed_lines = num_lines - *region.end;

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
            // Rotate selection to track content
            self.selection = self
                .selection
                .take()
                .and_then(|s| s.rotate(num_lines, num_cols, region, *positions as isize));

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

    pub fn clear_viewport(&mut self, template: &T) {
        // Determine how many lines to scroll up by.
        let end = Point { line: 0, col: self.num_cols() };
        let mut iter = self.iter_from(end);
        while let Some(cell) = iter.prev() {
            if !cell.is_empty() || iter.cur.line >= *self.lines {
                break;
            }
        }
        debug_assert!(iter.cur.line <= *self.lines);
        let positions = self.lines - iter.cur.line;
        let region = Line(0)..self.num_lines();

        // Reset display offset
        self.display_offset = 0;

        // Clear the viewport
        self.scroll_up(&region, positions, template);

        // Reset rotated lines
        for i in positions.0..self.lines.0 {
            self.raw[i].reset(&template);
        }
    }

    // Completely reset the grid state
    pub fn reset(&mut self, template: &T) {
        self.clear_history();

        // Reset all visible lines
        for row in 0..self.raw.len() {
            self.raw[row].reset(template);
        }

        self.display_offset = 0;
        self.selection = None;
    }
}

#[allow(clippy::len_without_is_empty)]
impl<T> Grid<T> {
    #[inline]
    pub fn num_lines(&self) -> Line {
        self.lines
    }

    pub fn display_iter(&self) -> DisplayIter<'_, T> {
        DisplayIter::new(self)
    }

    #[inline]
    pub fn num_cols(&self) -> Column {
        self.cols
    }

    pub fn clear_history(&mut self) {
        // Explicitly purge all lines from history
        self.raw.shrink_lines(self.history_size());
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
        T: Copy + GridCell,
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
    cur: Point<usize>,
}

impl<'a, T> GridIterator<'a, T> {
    pub fn point(&self) -> Point<usize> {
        self.cur
    }

    pub fn cell(&self) -> &'a T {
        &self.grid[self.cur.line][self.cur.col]
    }
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
impl<T> Index<Line> for Grid<T> {
    type Output = Row<T>;

    #[inline]
    fn index(&self, index: Line) -> &Row<T> {
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

impl<T> IndexMut<Line> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, index: Line) -> &mut Row<T> {
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
