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

use std::borrow::ToOwned;
use std::cmp::Ordering;
use std::iter::IntoIterator;
use std::ops::{Deref, DerefMut, Range, RangeTo, RangeFrom, RangeFull, Index, IndexMut};
use std::slice;
use std::collections::vec_deque::{self, VecDeque};

use index::{self, Point, AbsoluteLine, Line, Column, IndexRange, RangeInclusive};
use owned_slice::{self, TakeSlice};

/// Convert a type to a linear index range.
pub trait ToRange {
    fn to_range(&self) -> RangeInclusive<index::Linear>;
}

/// Bidirection iterator
pub trait BidirectionalIterator: Iterator {
    fn prev(&mut self) -> Option<Self::Item>;
}

pub struct Indexed<T> {
    pub line: Line,
    pub column: Column,
    pub inner: T
}

impl<T> Deref for Indexed<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.inner
    }
}


#[derive(Debug)]
pub enum MoveRegionError {
    AtTop,
    AtBottom
}

/// Represents a vertical movement within some abstract space
/// ie: the contents of the terminal window inside the scroll pane.
#[derive(Copy, Clone, Debug)]
pub enum Movement<T: Copy> {
    Up(T),
    Down(T),
    None
}

/// The scrollback system used for the grid. At the moment, there is either no
/// scrollback, or scrollback with a capped max number of lines. In the future,
/// this could be expanded to offer eg: unlimited scrollback.
#[derive(Copy, Clone, Debug)]
pub enum Scrollback {
    Disabled,
    MaxLines(index::AbsoluteLine)
}

// Internal struct used to keep track of scrollback state.
#[derive(Copy, Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct ScrollbackState {
    // Whether scrollback is enabled at all.. When disabled,
    // `max_lines` will be kept in sync with `lines` in the grid.
    enabled: bool,
    // Maximum number of lines in the total scrollback buffer.
    // Once this limit is reached, oldest elements will begin to be
    // removed from the `VecDeque` using `pop_front`
    max_lines: index::AbsoluteLine
}

fn default_scrollback_state() -> ScrollbackState {
    ScrollbackState { enabled: true, max_lines: AbsoluteLine(10000) }
}

/// Represents the terminal display contents
/// The grid itself has no knowledge of what our current scroll
/// position is. Instead, it just handles changes to the 'active region'
/// of the buffer (which is always the last `self.num_lines()` lines).
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Grid<T> {
    /// Lines in the grid. Each row holds a list of cells corresponding to the
    /// columns in that row.
    raw: VecDeque<Row<T>>,

    /// Number of columns
    cols: index::Column,

    /// Number of lines.
    lines: index::Line,

    /// The starting index for the visible region
    visible_region_start: index::AbsoluteLine,
    
    /// Scrollback config, ie: is it enabled, if so, how many lines
    #[serde(default = "default_scrollback_state")]
    scrollback: ScrollbackState,
}

pub struct GridIterator<'a, T: 'a> {
    grid: &'a Grid<T>,
    pub cur: Point,
}

impl<T: Clone> Grid<T> {
    pub fn new(lines: index::Line, cols: index::Column, scrollback: Scrollback, template: &T) -> Grid<T> {
        let mut raw = VecDeque::with_capacity(*lines);
        for _ in IndexRange(index::Line(0)..lines) {
            raw.push_back(Row::new(cols, template));
        }

        let scrollback_state = match scrollback {
            Scrollback::Disabled => ScrollbackState { enabled: false, max_lines: lines.to_absolute() },
            Scrollback::MaxLines(l) => ScrollbackState { enabled: true, max_lines: l }
        };

        Grid {
            raw: raw,
            cols: cols,
            lines: lines,
            visible_region_start: AbsoluteLine(0),
            scrollback: scrollback_state
        }
    }

    /// Resizes the visible region to the given number of lines and columns.
    pub fn resize(&mut self, lines: index::Line, cols: index::Column, template: &T) -> Movement<Line> {
        match self.cols.cmp(&cols) {
            Ordering::Less => self.grow_cols(cols, template),
            Ordering::Greater => self.shrink_cols(cols),
            Ordering::Equal => (),
        }

        match self.lines.cmp(&lines) {
            Ordering::Less => self.grow_lines(lines, template),
            Ordering::Greater => self.shrink_lines(lines),
            Ordering::Equal => Movement::None,
        }
    }
 
    /// Grows the active region/visible region to the given number of lines.
    /// The algorithm this uses should mimic Gnome Terminal's at the moment.
    /// That means that the contents bottom line of the screen should remain the same.
    ///
    /// The return value represents the direction and amount the 
    /// active region has moved - this can be either discarded or
    /// inspected ie: to keep the cursor in the right position.
    fn grow_lines(&mut self, lines: index::Line, template: &T) -> Movement<Line> {
        let old_start = self.active_region().start;
        
        // check if we actually need to add new lines
        if self.total_lines_in_buffer() < lines.to_absolute() {
            debug!("grow_lines: adding {} lines", lines.to_absolute() - self.total_lines_in_buffer());

            while self.total_lines_in_buffer() < lines.to_absolute() {
                self.raw.push_back(Row::new(self.cols, template));
            }
        }

        // move the start of the visible region up, so that the end
        // of the visible region is the same as it was before.
        let visible_shift = lines.to_absolute() - self.lines.to_absolute();
        let _ = self.move_visible_region_up(visible_shift);
    
        self.lines = lines;

        // calculate how much (if at all) the active region has moved up during the resize
        let new_start = self.active_region().start;
        let active_shift = old_start - new_start;
        Movement::Up(active_shift.to_relative())
    }

    fn grow_cols(&mut self, cols: index::Column, template: &T) {
        for row in self.all_lines_mut() {
            row.grow(cols, template);
        }

        self.cols = cols;
    }

}

impl<T> Grid<T> {
    /// Returns an iterator over only the active lines in the grid.
    /// TODO: use `impl Iterator<Item = &Row<T>>`
    #[inline]
    pub fn lines(&self) -> owned_slice::Iter<Grid<T>, Line, Row<T>> {
        self.index_range(Line(0)..self.num_lines()).iter()
    }

    /// Returns an mutable iterator over only the active lines in the grid.
    #[inline]
    pub fn lines_mut(&mut self) -> owned_slice::IterMut<Grid<T>, Line, Row<T>> {
        let region = Line(0)..self.num_lines();
        self.index_range_mut(region).iter_mut()
    }

    /// Returns an mutable iterator over only the active lines in the grid.
    #[inline]
    pub fn all_lines_mut(&mut self) -> vec_deque::IterMut<Row<T>> {
        self.raw.iter_mut()
    }

    /// The number of lines in the 'active region' of the terminal.
    /// This is effectively the height of the terminal window.
    #[inline]
    pub fn num_lines(&self) -> Line {
        self.lines
    }

    /// The number of columns in the 'active region' of the terminal.
    /// This is effectively the width of the terminal window.
    #[inline]
    pub fn num_cols(&self) -> Column {
        self.cols
    }

    /// The region that is not part of the scrollback buffer.
    /// It is still 'active' in the sense that it can be modified.
    #[inline]
    pub fn active_region(&self) -> Range<AbsoluteLine> {
        let end = self.total_lines_in_buffer();
        (end - self.num_lines().to_absolute())..end
    }

    /// Returns the region that is currently visible to the user.
    #[inline]
    pub fn visible_region(&self) -> Range<AbsoluteLine> {
        let end = self.visible_region_start + self.num_lines().to_absolute();
        self.visible_region_start..end
    }

    /// Moves the visible region up a relative amount.
    pub fn move_visible_region_up(&mut self, lines: AbsoluteLine) -> Result<(), MoveRegionError> {
        if self.visible_region_start == AbsoluteLine(0) {
            Err(MoveRegionError::AtTop)
        } else if self.visible_region_start < lines {
            // set the region to the top
            self.visible_region_start = AbsoluteLine(0);
            Ok(())
        } else {
            self.visible_region_start -= lines;
            Ok(())
        }
    }

    /// Moves the visible region down a relative amount.
    pub fn move_visible_region_down(&mut self, lines: AbsoluteLine) -> Result<(), MoveRegionError> {
        if self.visible_region().end == self.total_lines_in_buffer() {
            Err(MoveRegionError::AtBottom)
        } else if self.visible_region().end + lines >= self.total_lines_in_buffer() {
            self.visible_region_start = self.total_lines_in_buffer() - self.num_lines().to_absolute();
            Ok(())
        } else {
            self.visible_region_start += lines;
            Ok(())
        }
    }

    /// Moves the visible region to the very bottom. (eg: on new input)
    pub fn move_visible_region_to_bottom(&mut self) -> Result<(), MoveRegionError> {
        if self.visible_region().end == self.total_lines_in_buffer() {
            Err(MoveRegionError::AtBottom)
        } else {
            self.visible_region_start = self.total_lines_in_buffer() - self.num_lines().to_absolute();
            Ok(())
        }
    }

    /// The number of visible lines in the terminal.
    /// This is effectively the height of the terminal window.
    #[inline]
    pub fn total_lines_in_buffer(&self) -> AbsoluteLine {
        AbsoluteLine(self.raw.len())
    }

    /// TODO: Isn't this unused??
    /*pub fn iter_rows(&self) -> VDIter<Row<T>> {
        self.raw.iter()
    }*/

    /// Converts a line in the terminal to an
    /// absolute index into the scrollback buffer.
    pub fn active_to_absolute_line(&self, line: index::Line) -> AbsoluteLine {
        line.to_absolute() + self.active_region().start
    }

    /// An iterator with a point relative to the current viewable 'window'
    pub fn iter_from(&self, point: Point) -> GridIterator<T> {
        GridIterator {
            grid: self,
            cur: point,
        }
    }

    #[inline]
    pub fn scroll_down(&mut self, region: Range<index::Line>, positions: index::Line) {
        for line in IndexRange((region.start + positions)..region.end).rev() {
            self.swap_lines(line, line - positions);
        }
    }

    #[inline]
    pub fn scroll_up(&mut self, region: Range<index::Line>, positions: index::Line) {
        for line in IndexRange(region.start..(region.end - positions)) {
            self.swap_lines(line, line + positions);
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
        let src = self.active_to_absolute_line(src).0;
        let dst = self.active_to_absolute_line(dst).0;
        self.raw.swap(src, dst);
        /*use util::unlikely;

        unsafe {
            // check that src/dst are in bounds. Since index::Line newtypes usize,
            // we can assume values are positive.
            if unlikely(src >= self.lines) {
                panic!("swap_lines src out of bounds; len={}, src={}", self.raw.len(), src);
            }

            if unlikely(dst >= self.lines) {
                panic!("swap_lines dst out of bounds; len={}, dst={}", self.raw.len(), dst);
            }

            let src: *mut _ = self.raw.get_unchecked_mut(src.0);
            let dst: *mut _ = self.raw.get_unchecked_mut(dst.0);

            ::std::ptr::swap(src, dst);
        }*/
    }

    #[inline]
    pub fn clear<F: Fn(&mut T)>(&mut self, func: F) {
        let region = index::Line(0)..self.num_lines();
        self.clear_region(region, func);
    }

    /// Shrinks the active region/visible region to the given number of lines.
    /// The algorithm this uses should mimic Gnome Terminal's at the moment.
    /// That means that the contents bottom line of the screen should remain the same.
    ///
    /// The return value represents the direction and amount the 
    /// active region has moved - this can be either discarded or
    /// inspected ie: to keep the cursor in the right position.
    fn shrink_lines(&mut self, lines: index::Line) -> Movement<Line> {
        let shift = self.lines.to_absolute() - lines.to_absolute();
        
        self.lines = lines;

        // move the start of the visible region down, so that the end
        // of the visible region is the same as it was before.
        let _ = self.move_visible_region_down(shift);

        Movement::Down(shift.to_relative())
    }

    fn shrink_cols(&mut self, cols: index::Column) {
        for row in self.all_lines_mut() {
            row.shrink(cols);
        }

        self.cols = cols;
    }
}

impl<T: Default + Clone> Grid<T> {
    /// Inserts new rows into the datastructure
    /// This will allocate new Rows until `max_scrollback_lines` is reached,
    /// then it will reuse old rows.
    pub fn insert_new_lines<F>(&mut self, lines: index::Line, clear: F)
        where F: Fn(&mut T)
    {
        trace!("insert_new_lines: lines={}", lines);
        
        let was_at_end = self.visible_region().end == self.total_lines_in_buffer();

        let swap = self.total_lines_in_buffer() >= self.scrollback.max_lines;
        if swap {
            debug!("max scrollback lines reached, swapping with old rows");
            for _ in 0..lines.0 {
                let mut old_row = self.raw.pop_front().expect("empty cell buffer");
                for cell in &mut old_row {
                    clear(cell);
                }
                self.raw.push_back(old_row);
            }
            // After swapping lines, `visible_region_start` will be incorrect.
            // Therefore we then need to update the visible_region of the grid.
            // we don't care if it is already at the top, so we ignore the return value.
            let _ = self.move_visible_region_up(lines.to_absolute());
        } else {
            let cols = self.num_cols();
            for _ in 0..lines.0 {
                self.raw.push_back(Row::new(cols, &Default::default()));
            }
        }

        if was_at_end {
            trace!("keeping the visible region at the bottom");
            let _ = self.move_visible_region_to_bottom();
        }
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

/// Row indexing for the Grid
impl<T> Index<index::Line> for Grid<T> {
    type Output = Row<T>;

    #[inline]
    fn index(&self, index: index::Line) -> &Row<T> {
        let index = self.active_to_absolute_line(index).0;
        &self.raw[index]
    }
}

impl<T> IndexMut<index::Line> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, index: index::Line) -> &mut Row<T> {
        let index = self.active_to_absolute_line(index).0;
        &mut self.raw[index]
    }
}

/// Row slicing for the Grid.
impl<T> TakeSlice<Row<T>, Line> for Grid<T> {
    fn len(&self) -> Line {
        self.num_lines()
    }
}

/// Absolute slice for Grid<T>
impl<T> Grid<T> {
    /// Most the other functions of the grid deal only with the 'active region' of the buffer,
    /// which basically is the area of the buffer that is still being manipulated by apps,
    /// and isn't stored for the sole purpose of scrollback.
    ///
    /// This function allows access to a region in the buffer, indexed by an `AbsoluteLine`
    /// (ie: 0 means the very oldest line of scrollback). Note that this is read-only access,
    /// presumably for rendering purposes - the scrollback-area of the buffer
    /// should never be modified. 
    pub fn get_absolute_region(&self, region: Range<AbsoluteLine>) -> owned_slice::Slice<VecDeque<Row<T>>, usize, Row<T>> {
        owned_slice::Slice::new(&self.raw, region.start.0..region.end.0)
    }

    /// This function allows access to a specific line in the buffer, indexed by an `AbsoluteLine`
    /// (ie: 0 means the very oldest line of scrollback). Note that this is read-only access,
    /// presumably for rendering purposes - the scrollback-area of the buffer
    /// should never be modified.
    pub fn get_absolute_line(&self, line: AbsoluteLine) -> &Row<T> {
        &self.raw[line.0]
    }
}

/// Row-column indexing for the Grid
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

// -----------------------------------------------------------------------------
// Row
// -----------------------------------------------------------------------------

/// A row in the grid
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Row<T>(Vec<T>);

impl<T: Clone> Row<T> {
    pub fn new(columns: index::Column, template: &T) -> Row<T> {
        Row(vec![template.to_owned(); *columns])
    }

    pub fn grow(&mut self, cols: index::Column, template: &T) {
        while self.len() != *cols {
            self.push(template.to_owned());
        }
    }
}

impl<T> Row<T> {
    pub fn shrink(&mut self, cols: index::Column) {
        while self.len() != *cols {
            self.pop();
        }
    }

    #[inline]
    pub fn cells(&self) -> slice::Iter<T> {
        self.0.iter()
    }

    #[inline]
    pub fn cells_mut(&mut self) -> slice::IterMut<T> {
        self.0.iter_mut()
    }
}

impl<'a, T> IntoIterator for &'a Grid<T> {
    type Item = &'a Row<T>;
    type IntoIter = owned_slice::Iter<'a, Grid<T>, Line, Row<T>>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.lines()
    }
}

impl<'a, T> IntoIterator for &'a Row<T> {
    type Item = &'a T;
    type IntoIter = slice::Iter<'a, T>;

    #[inline]
    fn into_iter(self) -> slice::Iter<'a, T> {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut Row<T> {
    type Item = &'a mut T;
    type IntoIter = slice::IterMut<'a, T>;

    #[inline]
    fn into_iter(self) -> slice::IterMut<'a, T> {
        self.iter_mut()
    }
}

impl<T> Deref for Row<T> {
    type Target = Vec<T>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Row<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> Index<index::Column> for Row<T> {
    type Output = T;

    #[inline]
    fn index(&self, index: index::Column) -> &T {
        &self.0[index.0]
    }
}

impl<T> IndexMut<index::Column> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: index::Column) -> &mut T {
        &mut self.0[index.0]
    }
}

macro_rules! row_index_range {
    ($range:ty) => {
        impl<T> Index<$range> for Row<T> {
            type Output = [T];

            #[inline]
            fn index(&self, index: $range) -> &[T] {
                &self.0[index]
            }
        }

        impl<T> IndexMut<$range> for Row<T> {
            #[inline]
            fn index_mut(&mut self, index: $range) -> &mut [T] {
                &mut self.0[index]
            }
        }
    }
}

row_index_range!(Range<usize>);
row_index_range!(RangeTo<usize>);
row_index_range!(RangeFrom<usize>);
row_index_range!(RangeFull);

// -----------------------------------------------------------------------------
// Row ranges for Grid
// -----------------------------------------------------------------------------

/*type GridSlice<'a, T> = owned_slice::Slice<'a, VecDeque<Row<T>>, Row<T>>;
type GridSliceMut<'a, T> = owned_slice::SliceMut<'a, VecDeque<Row<T>>, Row<T>>;

impl<T> Grid<T> {
    pub fn index_range(&self, index: Range<Line>) -> GridSlice<T> {
        owned_slice::Slice::new(
            &self.raw,
            self.active_to_absolute_range(index)
        )
    }

    pub fn index_range_mut(&mut self, index: Range<Line>) -> GridSliceMut<T> {
        owned_slice::SliceMut::new(
            &mut self,
            self.active_to_absolute_range(index)
        )
    }

    pub fn index_range_to(&self, index: RangeTo<Line>) -> GridSlice<T> {
        self.index_range(Line(0)..index.end)
    }

    pub fn index_range_to_mut(&mut self, index: RangeTo<Line>) -> GridSliceMut<T> {
        self.index_range_mut(Line(0)..index.end)
    }

    pub fn index_range_from(&self, index: RangeFrom<Line>) -> GridSlice<T> {
        let len = self.num_lines();
        self.index_range(index.start..len)
    }

    pub fn index_range_from_mut(&mut self, index: RangeFrom<Line>) -> GridSliceMut<T> {
        let len = self.num_lines();
        self.index_range_mut(index.start..len)
    }
}*/

/*impl<T> Index<Range<index::Line>> for Grid<T> {
    type Output = [Row<T>];

    #[inline]
    fn index(&self, index: Range<index::Line>) -> &[Row<T>] {
        &self.raw[(index.start.0)..(index.end.0)]
    }
}

impl<T> IndexMut<Range<index::Line>> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, index: Range<index::Line>) -> &mut [Row<T>] {
        &mut self.raw[(index.start.0)..(index.end.0)]
    }
}

impl<T> Index<RangeTo<index::Line>> for Grid<T> {
    type Output = [Row<T>];

    #[inline]
    fn index(&self, index: RangeTo<index::Line>) -> &[Row<T>] {
        &self.raw[..(index.end.0)]
    }
}

impl<T> IndexMut<RangeTo<index::Line>> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, index: RangeTo<index::Line>) -> &mut [Row<T>] {
        &mut self.raw[..(index.end.0)]
    }
}

impl<T> Index<RangeFrom<index::Line>> for Grid<T> {
    type Output = [Row<T>];

    #[inline]
    fn index(&self, index: RangeFrom<index::Line>) -> &[Row<T>] {
        &self.raw[(index.start.0)..]
    }
}

impl<T> IndexMut<RangeFrom<index::Line>> for Grid<T> {
    #[inline]
    fn index_mut(&mut self, index: RangeFrom<index::Line>) -> &mut [Row<T>] {
        &mut self.raw[(index.start.0)..]
    }
}*/

// -----------------------------------------------------------------------------
// Column ranges for Row
// -----------------------------------------------------------------------------

impl<T> Index<Range<index::Column>> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, index: Range<index::Column>) -> &[T] {
        &self.0[(index.start.0)..(index.end.0)]
    }
}

impl<T> IndexMut<Range<index::Column>> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: Range<index::Column>) -> &mut [T] {
        &mut self.0[(index.start.0)..(index.end.0)]
    }
}

impl<T> Index<RangeTo<index::Column>> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, index: RangeTo<index::Column>) -> &[T] {
        &self.0[..(index.end.0)]
    }
}

impl<T> IndexMut<RangeTo<index::Column>> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: RangeTo<index::Column>) -> &mut [T] {
        &mut self.0[..(index.end.0)]
    }
}

impl<T> Index<RangeFrom<index::Column>> for Row<T> {
    type Output = [T];

    #[inline]
    fn index(&self, index: RangeFrom<index::Column>) -> &[T] {
        &self.0[(index.start.0)..]
    }
}

impl<T> IndexMut<RangeFrom<index::Column>> for Row<T> {
    #[inline]
    fn index_mut(&mut self, index: RangeFrom<index::Column>) -> &mut [T] {
        &mut self.0[(index.start.0)..]
    }
}

pub trait ClearRegion<R, T> {
    fn clear_region<F: Fn(&mut T)>(&mut self, region: R, func: F);
}

macro_rules! clear_region_impl {
    ($name:ident, $range:ty) => {
        impl<T> ClearRegion<$range, T> for Grid<T> {
            fn clear_region<F: Fn(&mut T)>(&mut self, region: $range, func: F) {
                for row in self.$name(region).iter_mut() {
                    for cell in row {
                        func(cell);
                    }
                }
            }
        }
    }
}

clear_region_impl!(index_range_mut, Range<index::Line>);
clear_region_impl!(index_range_to_mut, RangeTo<index::Line>);
clear_region_impl!(index_range_from_mut, RangeFrom<index::Line>);

#[cfg(test)]
mod tests {
    use super::{Grid, Scrollback, BidirectionalIterator};
    use index::{Point, Line, Column};
    #[test]
    fn grid_swap_lines_ok() {
        let mut grid = Grid::new(Line(10), Column(1), Scrollback::Disabled, &0);
        info!("");

        // swap test ends
        grid[Line(0)][Column(0)] = 1;
        grid[Line(9)][Column(0)] = 2;

        assert_eq!(grid[Line(0)][Column(0)], 1);
        assert_eq!(grid[Line(9)][Column(0)], 2);

        grid.swap_lines(Line(0), Line(9));

        assert_eq!(grid[Line(0)][Column(0)], 2);
        assert_eq!(grid[Line(9)][Column(0)], 1);

        // swap test mid
        grid[Line(4)][Column(0)] = 1;
        grid[Line(5)][Column(0)] = 2;

        info!("grid: {:?}", grid);

        assert_eq!(grid[Line(4)][Column(0)], 1);
        assert_eq!(grid[Line(5)][Column(0)], 2);

        grid.swap_lines(Line(4), Line(5));

        info!("grid: {:?}", grid);

        assert_eq!(grid[Line(4)][Column(0)], 2);
        assert_eq!(grid[Line(5)][Column(0)], 1);
    }

    #[test]
    #[should_panic]
    fn grid_swap_lines_oob1() {
        let mut grid = Grid::new(Line(10), Column(1), Scrollback::Disabled, &0);
        grid.swap_lines(Line(0), Line(10));
    }

    #[test]
    #[should_panic]
    fn grid_swap_lines_oob2() {
        let mut grid = Grid::new(Line(10), Column(1), Scrollback::Disabled, &0);
        grid.swap_lines(Line(10), Line(0));
    }

    #[test]
    #[should_panic]
    fn grid_swap_lines_oob3() {
        let mut grid = Grid::new(Line(10), Column(1), Scrollback::Disabled, &0);
        grid.swap_lines(Line(10), Line(10));
    }

    // Scroll up moves lines upwards
    #[test]
    fn scroll_up() {
        info!("");

        let mut grid = Grid::new(Line(10), Column(1), Scrollback::Disabled, &0);
        for i in 0..10 {
            grid[Line(i)][Column(0)] = i;
        }

        info!("grid: {:?}", grid);

        grid.scroll_up(Line(0)..Line(10), Line(2));

        info!("grid: {:?}", grid);

        let mut other = Grid::new(Line(10), Column(1), Scrollback::Disabled, &9);

        other[Line(0)][Column(0)] = 2;
        other[Line(1)][Column(0)] = 3;
        other[Line(2)][Column(0)] = 4;
        other[Line(3)][Column(0)] = 5;
        other[Line(4)][Column(0)] = 6;
        other[Line(5)][Column(0)] = 7;
        other[Line(6)][Column(0)] = 8;
        other[Line(7)][Column(0)] = 9;
        other[Line(8)][Column(0)] = 0;
        other[Line(9)][Column(0)] = 1;

        for i in 0..10 {
            assert_eq!(grid[Line(i)][Column(0)], other[Line(i)][Column(0)]);
        }
    }

    // Scroll down moves lines downwards
    #[test]
    fn scroll_down() {
        info!("");

        let mut grid = Grid::new(Line(10), Column(1), Scrollback::Disabled, &0);
        for i in 0..10 {
            grid[Line(i)][Column(0)] = i;
        }

        info!("grid: {:?}", grid);

        grid.scroll_down(Line(0)..Line(10), Line(2));

        info!("grid: {:?}", grid);

        let mut other = Grid::new(Line(10), Column(1), Scrollback::Disabled, &9);

        other[Line(0)][Column(0)] = 8;
        other[Line(1)][Column(0)] = 9;
        other[Line(2)][Column(0)] = 0;
        other[Line(3)][Column(0)] = 1;
        other[Line(4)][Column(0)] = 2;
        other[Line(5)][Column(0)] = 3;
        other[Line(6)][Column(0)] = 4;
        other[Line(7)][Column(0)] = 5;
        other[Line(8)][Column(0)] = 6;
        other[Line(9)][Column(0)] = 7;

        for i in 0..10 {
            assert_eq!(grid[Line(i)][Column(0)], other[Line(i)][Column(0)]);
        }
    }

    // Test that GridIterator works
    #[test]
    fn test_iter() {
        info!("");

        let mut grid = Grid::new(Line(5), Column(5), Scrollback::Disabled, &0);
        for i in 0..5 {
            for j in 0..5 {
                grid[Line(i)][Column(j)] = i*5 + j;
            }
        }

        info!("grid: {:?}", grid);

        let mut iter = grid.iter_from(Point {
            line: Line(0),
            col: Column(0),
        });

        assert_eq!(None, iter.prev());
        assert_eq!(Some(&1), iter.next());
        assert_eq!(Column(1), iter.cur.col);
        assert_eq!(Line(0), iter.cur.line);

        assert_eq!(Some(&2), iter.next());
        assert_eq!(Some(&3), iter.next());
        assert_eq!(Some(&4), iter.next());

        // test linewrapping
        assert_eq!(Some(&5), iter.next());
        assert_eq!(Column(0), iter.cur.col);
        assert_eq!(Line(1), iter.cur.line);

        assert_eq!(Some(&4), iter.prev());
        assert_eq!(Column(4), iter.cur.col);
        assert_eq!(Line(0), iter.cur.line);


        // test that iter ends at end of grid
        let mut final_iter = grid.iter_from(Point {
            line: Line(4),
            col: Column(4),
        });
        assert_eq!(None, final_iter.next());
        assert_eq!(Some(&23), final_iter.prev());
    }

}
