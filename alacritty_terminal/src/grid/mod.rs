//! A specialized 2D grid implementation optimized for use in a terminal.

use std::cmp::{max, min};
use std::ops::{Deref, Index, IndexMut, Range, RangeFrom, RangeFull, RangeTo};

use serde::{Deserialize, Serialize};

use crate::ansi::{CharsetIndex, StandardCharset};
use crate::index::{Column, IndexRange, Line, Point};
use crate::term::cell::{Cell, Flags};

pub mod resize;
mod row;
mod storage;
#[cfg(test)]
mod tests;

pub use self::row::Row;
use self::storage::Storage;

/// Bidirectional iterator.
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
        // Compare struct fields and check result of grid comparison.
        self.raw.eq(&other.raw)
            && self.cols.eq(&other.cols)
            && self.lines.eq(&other.lines)
            && self.display_offset.eq(&other.display_offset)
    }
}

pub trait GridCell {
    fn is_empty(&self) -> bool;
    fn flags(&self) -> &Flags;
    fn flags_mut(&mut self) -> &mut Flags;

    /// Fast equality approximation.
    ///
    /// This is a faster alternative to [`PartialEq`],
    /// but might report unequal cells as equal.
    fn fast_eq(&self, other: Self) -> bool;
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct Cursor {
    /// The location of this cursor.
    pub point: Point,

    /// Template cell when using this cursor.
    pub template: Cell,

    /// Currently configured graphic character sets.
    pub charsets: Charsets,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct Charsets([StandardCharset; 4]);

impl Index<CharsetIndex> for Charsets {
    type Output = StandardCharset;

    fn index(&self, index: CharsetIndex) -> &StandardCharset {
        &self.0[index as usize]
    }
}

impl IndexMut<CharsetIndex> for Charsets {
    fn index_mut(&mut self, index: CharsetIndex) -> &mut StandardCharset {
        &mut self.0[index as usize]
    }
}

/// Grid based terminal content storage.
///
/// ```notrust
/// ┌─────────────────────────┐  <-- max_scroll_limit + lines
/// │                         │
/// │      UNINITIALIZED      │
/// │                         │
/// ├─────────────────────────┤  <-- self.raw.inner.len()
/// │                         │
/// │      RESIZE BUFFER      │
/// │                         │
/// ├─────────────────────────┤  <-- self.history_size() + lines
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
    /// Current cursor for writing data.
    #[serde(skip)]
    pub cursor: Cursor,

    /// Last saved cursor.
    #[serde(skip)]
    pub saved_cursor: Cursor,

    /// Lines in the grid. Each row holds a list of cells corresponding to the
    /// columns in that row.
    raw: Storage<T>,

    /// Number of columns.
    cols: Column,

    /// Number of visible lines.
    lines: Line,

    /// Offset of displayed area.
    ///
    /// If the displayed region isn't at the bottom of the screen, it stays
    /// stationary while more text is emitted. The scrolling implementation
    /// updates this offset accordingly.
    display_offset: usize,

    /// Maximum number of lines in history.
    max_scroll_limit: usize,
}

#[derive(Debug, Copy, Clone)]
pub enum Scroll {
    Lines(isize),
    PageUp,
    PageDown,
    Top,
    Bottom,
}

impl<T: GridCell + Default + PartialEq + Copy> Grid<T> {
    pub fn new(lines: Line, cols: Column, max_scroll_limit: usize, template: T) -> Grid<T> {
        Grid {
            raw: Storage::with_capacity(lines, Row::new(cols, template)),
            max_scroll_limit,
            display_offset: 0,
            saved_cursor: Cursor::default(),
            cursor: Cursor::default(),
            lines,
            cols,
        }
    }

    /// Clamp a buffer point to the visible region.
    pub fn clamp_buffer_to_visible(&self, point: Point<usize>) -> Point {
        if point.line < self.display_offset {
            Point::new(self.lines - 1, self.cols - 1)
        } else if point.line >= self.display_offset + self.lines.0 {
            Point::new(Line(0), Column(0))
        } else {
            // Since edge-cases are handled, conversion is identical as visible to buffer.
            self.visible_to_buffer(point.into()).into()
        }
    }

    /// Convert viewport relative point to global buffer indexing.
    pub fn visible_to_buffer(&self, point: Point) -> Point<usize> {
        Point { line: self.lines.0 + self.display_offset - point.line.0 - 1, col: point.col }
    }

    /// Update the size of the scrollback history.
    pub fn update_history(&mut self, history_size: usize) {
        let current_history_size = self.history_size();
        if current_history_size > history_size {
            self.raw.shrink_lines(current_history_size - history_size);
        }
        self.display_offset = min(self.display_offset, history_size);
        self.max_scroll_limit = history_size;
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

    fn increase_scroll_limit(&mut self, count: usize, template: T) {
        let count = min(count, self.max_scroll_limit - self.history_size());
        if count != 0 {
            self.raw.initialize(count, template, self.cols);
        }
    }

    fn decrease_scroll_limit(&mut self, count: usize) {
        let count = min(count, self.history_size());
        if count != 0 {
            self.raw.shrink_lines(min(count, self.history_size()));
            self.display_offset = min(self.display_offset, self.history_size());
        }
    }

    #[inline]
    pub fn scroll_down(&mut self, region: &Range<Line>, positions: Line, template: T) {
        // Whether or not there is a scrolling region active, as long as it
        // starts at the top, we can do a full rotation which just involves
        // changing the start index.
        //
        // To accommodate scroll regions, rows are reordered at the end.
        if region.start == Line(0) {
            // Rotate the entire line buffer. If there's a scrolling region
            // active, the bottom lines are restored in the next step.
            self.raw.rotate_up(*positions);

            self.decrease_scroll_limit(*positions);

            // Now, restore any scroll region lines.
            let lines = self.lines;
            for i in IndexRange(region.end..lines) {
                self.raw.swap_lines(i, i + positions);
            }

            // Finally, reset recycled lines.
            for i in IndexRange(Line(0)..positions) {
                self.raw[i].reset(template);
            }
        } else {
            // Subregion rotation.
            for line in IndexRange((region.start + positions)..region.end).rev() {
                self.raw.swap_lines(line, line - positions);
            }

            for line in IndexRange(region.start..(region.start + positions)) {
                self.raw[line].reset(template);
            }
        }
    }

    /// Move lines at the bottom towards the top.
    ///
    /// This is the performance-sensitive part of scrolling.
    pub fn scroll_up(&mut self, region: &Range<Line>, positions: Line, template: T) {
        let num_lines = self.num_lines().0;

        if region.start == Line(0) {
            // Update display offset when not pinned to active area.
            if self.display_offset != 0 {
                self.display_offset = min(self.display_offset + *positions, self.max_scroll_limit);
            }

            self.increase_scroll_limit(*positions, template);

            // Rotate the entire line buffer. If there's a scrolling region
            // active, the bottom lines are restored in the next step.
            self.raw.rotate(-(*positions as isize));

            // This next loop swaps "fixed" lines outside of a scroll region
            // back into place after the rotation. The work is done in buffer-
            // space rather than terminal-space to avoid redundant
            // transformations.
            let fixed_lines = num_lines - *region.end;

            for i in 0..fixed_lines {
                self.raw.swap(i, i + *positions);
            }

            // Finally, reset recycled lines.
            //
            // Recycled lines are just above the end of the scrolling region.
            for i in 0..*positions {
                self.raw[i + fixed_lines].reset(template);
            }
        } else {
            // Subregion rotation.
            for line in IndexRange(region.start..(region.end - positions)) {
                self.raw.swap_lines(line, line + positions);
            }

            // Clear reused lines.
            for line in IndexRange((region.end - positions)..region.end) {
                self.raw[line].reset(template);
            }
        }
    }

    pub fn clear_viewport(&mut self, template: T) {
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

        // Reset display offset.
        self.display_offset = 0;

        // Clear the viewport.
        self.scroll_up(&region, positions, template);

        // Reset rotated lines.
        for i in positions.0..self.lines.0 {
            self.raw[i].reset(template);
        }
    }

    /// Completely reset the grid state.
    pub fn reset(&mut self, template: T) {
        self.clear_history();

        // Reset all visible lines.
        for row in 0..self.raw.len() {
            self.raw[row].reset(template);
        }

        self.saved_cursor = Cursor::default();
        self.cursor = Cursor::default();
        self.display_offset = 0;
    }
}

#[allow(clippy::len_without_is_empty)]
impl<T> Grid<T> {
    #[inline]
    pub fn num_lines(&self) -> Line {
        self.lines
    }

    #[inline]
    pub fn display_iter(&self) -> DisplayIter<'_, T> {
        DisplayIter::new(self)
    }

    #[inline]
    pub fn num_cols(&self) -> Column {
        self.cols
    }

    #[inline]
    pub fn clear_history(&mut self) {
        // Explicitly purge all lines from history.
        self.raw.shrink_lines(self.history_size());
    }

    /// Total number of lines in the buffer, this includes scrollback + visible lines.
    #[inline]
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    #[inline]
    pub fn history_size(&self) -> usize {
        self.raw.len() - *self.lines
    }

    /// This is used only for initializing after loading ref-tests.
    #[inline]
    pub fn initialize_all(&mut self, template: T)
    where
        T: Copy + GridCell,
    {
        // Remove all cached lines to clear them of any content.
        self.truncate();

        // Initialize everything with empty new lines.
        self.raw.initialize(self.max_scroll_limit - self.history_size(), template, self.cols);
    }

    /// This is used only for truncating before saving ref-tests.
    #[inline]
    pub fn truncate(&mut self) {
        self.raw.truncate();
    }

    #[inline]
    pub fn iter_from(&self, point: Point<usize>) -> GridIterator<'_, T> {
        GridIterator { grid: self, cur: point }
    }

    #[inline]
    pub fn display_offset(&self) -> usize {
        self.display_offset
    }

    #[inline]
    pub fn cursor_cell(&mut self) -> &mut T {
        let point = self.cursor.point;
        &mut self[&point]
    }
}

pub struct GridIterator<'a, T> {
    /// Immutable grid reference.
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

/// Index active region by line.
impl<T> Index<Line> for Grid<T> {
    type Output = Row<T>;

    #[inline]
    fn index(&self, index: Line) -> &Row<T> {
        &self.raw[index]
    }
}

/// Index with buffer offset.
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

/// A subset of lines in the grid.
///
/// May be constructed using Grid::region(..).
pub struct Region<'a, T> {
    start: Line,
    end: Line,
    raw: &'a Storage<T>,
}

/// A mutable subset of lines in the grid.
///
/// May be constructed using Grid::region_mut(..).
pub struct RegionMut<'a, T> {
    start: Line,
    end: Line,
    raw: &'a mut Storage<T>,
}

impl<'a, T> RegionMut<'a, T> {
    /// Call the provided function for every item in this region.
    pub fn each<F: Fn(&mut T)>(self, func: F) {
        for row in self {
            for item in row {
                func(item)
            }
        }
    }
}

pub trait IndexRegion<I, T> {
    /// Get an immutable region of Self.
    fn region(&self, _: I) -> Region<'_, T>;

    /// Get a mutable region of Self.
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

/// Iterates over the visible area accounting for buffer transform.
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

        // Update line/col to point to next item.
        self.col += 1;
        if self.col == self.grid.num_cols() && self.offset != self.limit {
            self.offset -= 1;

            self.col = Column(0);
            self.line = Line(*self.grid.lines - 1 - (self.offset - self.limit));
        }

        item
    }
}
