//! A specialized 2D grid implementation optimized for use in a terminal.

use std::cmp::{max, min};
use std::iter::{Map, TakeWhile};
use std::ops::{Bound, Deref, Index, IndexMut, Range, RangeBounds, RangeInclusive};

use serde::{Deserialize, Serialize};

use crate::ansi::{CharsetIndex, StandardCharset};
use crate::index::{Column, IndexRange, Line, Point};
use crate::term::cell::{Flags, ResetDiscriminant};

pub mod resize;
mod row;
mod storage;
#[cfg(test)]
mod tests;

pub use self::row::Row;
use self::storage::Storage;

pub trait GridCell: Sized {
    /// Check if the cell contains any content.
    fn is_empty(&self) -> bool;

    /// Perform an opinionated cell reset based on a template cell.
    fn reset(&mut self, template: &Self);

    fn flags(&self) -> &Flags;
    fn flags_mut(&mut self) -> &mut Flags;
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Cursor<T> {
    /// The location of this cursor.
    pub point: Point,

    /// Template cell when using this cursor.
    pub template: T,

    /// Currently configured graphic character sets.
    pub charsets: Charsets,

    /// Tracks if the next call to input will need to first handle wrapping.
    ///
    /// This is true after the last column is set with the input function. Any function that
    /// implicitly sets the line or column needs to set this to false to avoid wrapping twice.
    ///
    /// Tracking `input_needs_wrap` makes it possible to not store a cursor position that exceeds
    /// the number of columns, which would lead to index out of bounds when interacting with arrays
    /// without sanitization.
    pub input_needs_wrap: bool,
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

#[derive(Debug, Copy, Clone)]
pub enum Scroll {
    Delta(isize),
    PageUp,
    PageDown,
    Top,
    Bottom,
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
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Grid<T> {
    /// Current cursor for writing data.
    #[serde(skip)]
    pub cursor: Cursor<T>,

    /// Last saved cursor.
    #[serde(skip)]
    pub saved_cursor: Cursor<T>,

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

impl<T: GridCell + Default + PartialEq + Clone> Grid<T> {
    pub fn new(lines: Line, cols: Column, max_scroll_limit: usize) -> Grid<T> {
        Grid {
            raw: Storage::with_capacity(lines, cols),
            max_scroll_limit,
            display_offset: 0,
            saved_cursor: Cursor::default(),
            cursor: Cursor::default(),
            lines,
            cols,
        }
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
        self.display_offset = match scroll {
            Scroll::Delta(count) => min(
                max((self.display_offset as isize) + count, 0isize) as usize,
                self.history_size(),
            ),
            Scroll::PageUp => min(self.display_offset + self.lines.0, self.history_size()),
            Scroll::PageDown => self.display_offset.saturating_sub(self.lines.0),
            Scroll::Top => self.history_size(),
            Scroll::Bottom => 0,
        };
    }

    fn increase_scroll_limit(&mut self, count: usize) {
        let count = min(count, self.max_scroll_limit - self.history_size());
        if count != 0 {
            self.raw.initialize(count, self.cols);
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
    pub fn scroll_down<D>(&mut self, region: &Range<Line>, positions: Line)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        let screen_lines = self.screen_lines().0;

        // When rotating the entire region, just reset everything.
        if positions >= region.end - region.start {
            for i in region.start.0..region.end.0 {
                let index = screen_lines - i - 1;
                self.raw[index].reset(&self.cursor.template);
            }

            return;
        }

        // Which implementation we can use depends on the existence of a scrollback history.
        //
        // Since a scrollback history prevents us from rotating the entire buffer downwards, we
        // instead have to rely on a slower, swap-based implementation.
        if self.max_scroll_limit == 0 {
            // Swap the lines fixed at the bottom to their target positions after rotation.
            //
            // Since we've made sure that the rotation will never rotate away the entire region, we
            // know that the position of the fixed lines before the rotation must already be
            // visible.
            //
            // We need to start from the top, to make sure the fixed lines aren't swapped with each
            // other.
            let fixed_lines = screen_lines - region.end.0;
            for i in (0..fixed_lines).rev() {
                self.raw.swap(i, i + positions.0);
            }

            // Rotate the entire line buffer downward.
            self.raw.rotate_down(*positions);

            // Ensure all new lines are fully cleared.
            for i in 0..positions.0 {
                let index = screen_lines - i - 1;
                self.raw[index].reset(&self.cursor.template);
            }

            // Swap the fixed lines at the top back into position.
            for i in 0..region.start.0 {
                let index = screen_lines - i - 1;
                self.raw.swap(index, index - positions.0);
            }
        } else {
            // Subregion rotation.
            for line in IndexRange((region.start + positions)..region.end).rev() {
                self.raw.swap_lines(line, line - positions);
            }

            for line in IndexRange(region.start..(region.start + positions)) {
                self.raw[line].reset(&self.cursor.template);
            }
        }
    }

    /// Move lines at the bottom toward the top.
    ///
    /// This is the performance-sensitive part of scrolling.
    pub fn scroll_up<D>(&mut self, region: &Range<Line>, positions: Line)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        let screen_lines = self.screen_lines().0;

        // When rotating the entire region with fixed lines at the top, just reset everything.
        if positions >= region.end - region.start && region.start != Line(0) {
            for i in region.start.0..region.end.0 {
                let index = screen_lines - i - 1;
                self.raw[index].reset(&self.cursor.template);
            }

            return;
        }

        // Update display offset when not pinned to active area.
        if self.display_offset != 0 {
            self.display_offset = min(self.display_offset + *positions, self.max_scroll_limit);
        }

        // Create scrollback for the new lines.
        self.increase_scroll_limit(*positions);

        // Swap the lines fixed at the top to their target positions after rotation.
        //
        // Since we've made sure that the rotation will never rotate away the entire region, we
        // know that the position of the fixed lines before the rotation must already be
        // visible.
        //
        // We need to start from the bottom, to make sure the fixed lines aren't swapped with each
        // other.
        for i in (0..region.start.0).rev() {
            let index = screen_lines - i - 1;
            self.raw.swap(index, index - positions.0);
        }

        // Rotate the entire line buffer upward.
        self.raw.rotate(-(positions.0 as isize));

        // Ensure all new lines are fully cleared.
        for i in 0..positions.0 {
            self.raw[i].reset(&self.cursor.template);
        }

        // Swap the fixed lines at the bottom back into position.
        let fixed_lines = screen_lines - region.end.0;
        for i in 0..fixed_lines {
            self.raw.swap(i, i + positions.0);
        }
    }

    pub fn clear_viewport<D>(&mut self)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        // Determine how many lines to scroll up by.
        let end = Point { line: 0, column: self.cols() };
        let mut iter = self.iter_from(end);
        while let Some(cell) = iter.prev() {
            if !cell.is_empty() || cell.point.line >= *self.lines {
                break;
            }
        }
        debug_assert!(iter.point.line <= *self.lines);
        let positions = self.lines - iter.point.line;
        let region = Line(0)..self.screen_lines();

        // Reset display offset.
        self.display_offset = 0;

        // Clear the viewport.
        self.scroll_up(&region, positions);

        // Reset rotated lines.
        for i in positions.0..self.lines.0 {
            self.raw[i].reset(&self.cursor.template);
        }
    }

    /// Completely reset the grid state.
    pub fn reset<D>(&mut self)
    where
        T: ResetDiscriminant<D>,
        D: PartialEq,
    {
        self.clear_history();

        self.saved_cursor = Cursor::default();
        self.cursor = Cursor::default();
        self.display_offset = 0;

        // Reset all visible lines.
        for row in 0..self.raw.len() {
            self.raw[row].reset(&self.cursor.template);
        }
    }
}

impl<T> Grid<T> {
    /// Reset a visible region within the grid.
    pub fn reset_region<D, R: RangeBounds<Line>>(&mut self, bounds: R)
    where
        T: ResetDiscriminant<D> + GridCell + Clone + Default,
        D: PartialEq,
    {
        let start = match bounds.start_bound() {
            Bound::Included(line) => *line,
            Bound::Excluded(line) => *line + 1,
            Bound::Unbounded => Line(0),
        };

        let end = match bounds.end_bound() {
            Bound::Included(line) => *line + 1,
            Bound::Excluded(line) => *line,
            Bound::Unbounded => self.screen_lines(),
        };

        debug_assert!(start < self.screen_lines());
        debug_assert!(end <= self.screen_lines());

        for row in start.0..end.0 {
            self.raw[Line(row)].reset(&self.cursor.template);
        }
    }

    /// Clamp a buffer point to the visible region.
    pub fn clamp_buffer_to_visible(&self, point: Point<usize>) -> Point {
        if point.line < self.display_offset {
            Point::new(self.lines - 1, self.cols - 1)
        } else if point.line >= self.display_offset + self.lines.0 {
            Point::new(Line(0), Column(0))
        } else {
            // Since edgecases are handled, conversion is identical as visible to buffer.
            self.visible_to_buffer(point.into()).into()
        }
    }

    // Clamp a buffer point based range to the viewport.
    //
    // This will make sure the content within the range is visible and return `None` whenever the
    // entire range is outside the visible region.
    pub fn clamp_buffer_range_to_visible(
        &self,
        range: &RangeInclusive<Point<usize>>,
    ) -> Option<RangeInclusive<Point>> {
        let start = range.start();
        let end = range.end();

        // Check if the range is completely offscreen
        let viewport_end = self.display_offset;
        let viewport_start = viewport_end + self.lines.0 - 1;
        if end.line > viewport_start || start.line < viewport_end {
            return None;
        }

        let start = self.clamp_buffer_to_visible(*start);
        let end = self.clamp_buffer_to_visible(*end);

        Some(start..=end)
    }

    /// Convert viewport relative point to global buffer indexing.
    #[inline]
    pub fn visible_to_buffer(&self, point: Point) -> Point<usize> {
        Point { line: self.lines.0 + self.display_offset - point.line.0 - 1, column: point.column }
    }

    #[inline]
    pub fn clear_history(&mut self) {
        // Explicitly purge all lines from history.
        self.raw.shrink_lines(self.history_size());
    }

    /// This is used only for initializing after loading ref-tests.
    #[inline]
    pub fn initialize_all(&mut self)
    where
        T: GridCell + Clone + Default,
    {
        // Remove all cached lines to clear them of any content.
        self.truncate();

        // Initialize everything with empty new lines.
        self.raw.initialize(self.max_scroll_limit - self.history_size(), self.cols);
    }

    /// This is used only for truncating before saving ref-tests.
    #[inline]
    pub fn truncate(&mut self) {
        self.raw.truncate();
    }

    #[inline]
    pub fn iter_from(&self, point: Point<usize>) -> GridIterator<'_, T> {
        GridIterator { grid: self, point }
    }

    /// Iterator over all visible cells.
    #[inline]
    pub fn display_iter(&self) -> DisplayIter<'_, T> {
        let start = Point::new(self.display_offset + self.lines.0, self.cols() - 1);
        let end = Point::new(self.display_offset, self.cols());

        let iter = GridIterator { grid: self, point: start };

        let display_offset = self.display_offset;
        let lines = self.lines.0;

        let take_while: DisplayIterTakeFun<'_, T> =
            Box::new(move |indexed: &Indexed<&T>| indexed.point <= end);
        let map: DisplayIterMapFun<'_, T> = Box::new(move |indexed: Indexed<&T>| {
            let line = Line(lines + display_offset - indexed.point.line - 1);
            Indexed { point: Point::new(line, indexed.point.column), cell: indexed.cell }
        });
        iter.take_while(take_while).map(map)
    }

    #[inline]
    pub fn display_offset(&self) -> usize {
        self.display_offset
    }

    #[inline]
    pub fn cursor_cell(&mut self) -> &mut T {
        let point = self.cursor.point;
        &mut self[point.line][point.column]
    }
}

impl<T: PartialEq> PartialEq for Grid<T> {
    fn eq(&self, other: &Self) -> bool {
        // Compare struct fields and check result of grid comparison.
        self.raw.eq(&other.raw)
            && self.cols.eq(&other.cols)
            && self.lines.eq(&other.lines)
            && self.display_offset.eq(&other.display_offset)
    }
}

impl<T> Index<Line> for Grid<T> {
    type Output = Row<T>;

    #[inline]
    fn index(&self, index: Line) -> &Row<T> {
        &self.raw[index]
    }
}

impl<T> IndexMut<Line> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, index: Line) -> &mut Row<T> {
        &mut self.raw[index]
    }
}

impl<T> Index<usize> for Grid<T> {
    type Output = Row<T>;

    #[inline]
    fn index(&self, index: usize) -> &Row<T> {
        &self.raw[index]
    }
}

impl<T> IndexMut<usize> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Row<T> {
        &mut self.raw[index]
    }
}

impl<T> Index<Point<usize>> for Grid<T> {
    type Output = T;

    #[inline]
    fn index(&self, point: Point<usize>) -> &T {
        &self[point.line][point.column]
    }
}

impl<T> IndexMut<Point<usize>> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, point: Point<usize>) -> &mut T {
        &mut self[point.line][point.column]
    }
}

impl<T> Index<Point> for Grid<T> {
    type Output = T;

    #[inline]
    fn index(&self, point: Point) -> &T {
        &self[point.line][point.column]
    }
}

impl<T> IndexMut<Point> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, point: Point) -> &mut T {
        &mut self[point.line][point.column]
    }
}

/// Grid dimensions.
pub trait Dimensions {
    /// Total number of lines in the buffer, this includes scrollback and visible lines.
    fn total_lines(&self) -> usize;

    /// Height of the viewport in lines.
    fn screen_lines(&self) -> Line;

    /// Width of the terminal in columns.
    fn cols(&self) -> Column;

    /// Number of invisible lines part of the scrollback history.
    #[inline]
    fn history_size(&self) -> usize {
        self.total_lines() - self.screen_lines().0
    }
}

impl<G> Dimensions for Grid<G> {
    #[inline]
    fn total_lines(&self) -> usize {
        self.raw.len()
    }

    #[inline]
    fn screen_lines(&self) -> Line {
        self.lines
    }

    #[inline]
    fn cols(&self) -> Column {
        self.cols
    }
}

#[cfg(test)]
impl Dimensions for (Line, Column) {
    fn total_lines(&self) -> usize {
        *self.0
    }

    fn screen_lines(&self) -> Line {
        self.0
    }

    fn cols(&self) -> Column {
        self.1
    }
}

#[derive(Debug, PartialEq)]
pub struct Indexed<T, L = usize> {
    pub point: Point<L>,
    pub cell: T,
}

impl<T, L> Deref for Indexed<T, L> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.cell
    }
}

/// Grid cell iterator.
pub struct GridIterator<'a, T> {
    /// Immutable grid reference.
    grid: &'a Grid<T>,

    /// Current position of the iterator within the grid.
    point: Point<usize>,
}

impl<'a, T> GridIterator<'a, T> {
    /// Current iteratior position.
    pub fn point(&self) -> Point<usize> {
        self.point
    }

    /// Cell at the current iteratior position.
    pub fn cell(&self) -> &'a T {
        &self.grid[self.point]
    }
}

impl<'a, T> Iterator for GridIterator<'a, T> {
    type Item = Indexed<&'a T>;

    fn next(&mut self) -> Option<Self::Item> {
        let last_col = self.grid.cols() - 1;
        match self.point {
            Point { line, column: col } if line == 0 && col == last_col => return None,
            Point { column: col, .. } if (col == last_col) => {
                self.point.line -= 1;
                self.point.column = Column(0);
            },
            _ => self.point.column += Column(1),
        }

        Some(Indexed { cell: &self.grid[self.point], point: self.point })
    }
}

/// Bidirectional iterator.
pub trait BidirectionalIterator: Iterator {
    fn prev(&mut self) -> Option<Self::Item>;
}

impl<'a, T> BidirectionalIterator for GridIterator<'a, T> {
    fn prev(&mut self) -> Option<Self::Item> {
        let last_col = self.grid.cols() - 1;

        match self.point {
            Point { line, column: Column(0) } if line == self.grid.total_lines() - 1 => {
                return None
            },
            Point { column: Column(0), .. } => {
                self.point.line += 1;
                self.point.column = last_col;
            },
            _ => self.point.column -= Column(1),
        }

        Some(Indexed { cell: &self.grid[self.point], point: self.point })
    }
}

pub type DisplayIter<'a, T> =
    Map<TakeWhile<GridIterator<'a, T>, DisplayIterTakeFun<'a, T>>, DisplayIterMapFun<'a, T>>;
type DisplayIterTakeFun<'a, T> = Box<dyn Fn(&Indexed<&'a T>) -> bool>;
type DisplayIterMapFun<'a, T> = Box<dyn FnMut(Indexed<&'a T>) -> Indexed<&'a T, Line>>;
