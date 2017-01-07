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
//
//! Exports the `Term` type which is a high-level API for the Grid
use std::ops::{Deref, Range};
use std::ptr;
use std::cmp;
use std::io;

use ansi::{self, Color, NamedColor, Attr, Handler};
use grid::{Grid, ClearRegion, ToRange};
use index::{self, Point, Column, Line, Linear, IndexRange, Contains, RangeInclusive};
use selection::{Span, Selection};

pub mod cell;
pub use self::cell::Cell;
use self::cell::LineLength;

/// Iterator that yields cells needing render
///
/// Yields cells that require work to be displayed (that is, not a an empty
/// background cell). Additionally, this manages some state of the grid only
/// relevant for rendering like temporarily changing the cell with the cursor.
///
/// This manages the cursor during a render. The cursor location is inverted to
/// draw it, and reverted after drawing to maintain state.
pub struct RenderableCellsIter<'a> {
    grid: &'a mut Grid<Cell>,
    cursor: &'a Point,
    mode: TermMode,
    line: Line,
    column: Column,
    selection: Option<RangeInclusive<index::Linear>>,
}

impl<'a> RenderableCellsIter<'a> {
    /// Create the renderable cells iterator
    ///
    /// The cursor and terminal mode are required for properly displaying the
    /// cursor.
    fn new<'b>(
        grid: &'b mut Grid<Cell>,
        cursor: &'b Point,
        mode: TermMode,
        selection: &Selection,
    ) -> RenderableCellsIter<'b> {
        let selection = selection.span()
            .map(|span| span.to_range(grid.num_cols()));

        RenderableCellsIter {
            grid: grid,
            cursor: cursor,
            mode: mode,
            line: Line(0),
            column: Column(0),
            selection: selection,
        }.initialize()
    }

    fn initialize(self) -> Self {
        if self.cursor_is_visible() {
            self.grid[self.cursor].swap_fg_and_bg();
        }

        self
    }

    /// Check if the cursor should be rendered.
    #[inline]
    fn cursor_is_visible(&self) -> bool {
        self.mode.contains(mode::SHOW_CURSOR) && self.grid.contains(self.cursor)
    }
}

impl<'a> Drop for RenderableCellsIter<'a> {
    /// Resets temporary render state on the grid
    fn drop(&mut self) {
        if self.cursor_is_visible() {
            self.grid[self.cursor].swap_fg_and_bg();
        }
    }
}

pub struct IndexedCell {
    pub line: Line,
    pub column: Column,
    pub inner: Cell
}

impl Deref for IndexedCell {
    type Target = Cell;

    #[inline]
    fn deref(&self) -> &Cell {
        &self.inner
    }
}

impl<'a> Iterator for RenderableCellsIter<'a> {
    type Item = IndexedCell;

    /// Gets the next renderable cell
    ///
    /// Skips empty (background) cells and applies any flags to the cell state
    /// (eg. invert fg and bg colors).
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        while self.line < self.grid.num_lines() {
            while self.column < self.grid.num_cols() {
                // Grab current state for this iteration
                let line = self.line;
                let column = self.column;
                let cell = &self.grid[line][column];

                let index = Linear(line.0 * self.grid.num_cols().0 + column.0);

                // Update state for next iteration
                self.column += 1;

                let selected = self.selection.as_ref()
                    .map(|range| range.contains_(index))
                    .unwrap_or(false);

                // Skip empty cells
                if cell.is_empty() && !selected {
                    continue;
                }

                // fg, bg are dependent on INVERSE flag
                let invert = cell.flags.contains(cell::INVERSE) || selected;

                let (fg, bg) = if invert {
                    (&cell.bg, &cell.fg)
                } else {
                    (&cell.fg, &cell.bg)
                };

                return Some(IndexedCell {
                    line: line,
                    column: column,
                    inner: Cell {
                        flags: cell.flags,
                        c: cell.c,
                        fg: *fg,
                        bg: *bg,
                    }
                })
            }

            self.column = Column(0);
            self.line += 1;
        }

        None
    }
}

/// coerce val to be between min and max
#[inline]
fn limit<T: PartialOrd + Ord>(val: T, min: T, max: T) -> T {
    cmp::min(cmp::max(min, val), max)
}

pub mod mode {
    bitflags! {
        pub flags TermMode: u8 {
            const SHOW_CURSOR         = 0b00000001,
            const APP_CURSOR          = 0b00000010,
            const APP_KEYPAD          = 0b00000100,
            const MOUSE_REPORT_CLICK  = 0b00001000,
            const BRACKETED_PASTE     = 0b00010000,
            const SGR_MOUSE           = 0b00100000,
            const MOUSE_MOTION        = 0b01000000,
            const ANY                 = 0b11111111,
            const NONE                = 0b00000000,
        }
    }

    impl Default for TermMode {
        fn default() -> TermMode {
            SHOW_CURSOR
        }
    }
}

pub use self::mode::TermMode;

pub const TAB_SPACES: usize = 8;

pub struct Term {
    /// The grid
    grid: Grid<Cell>,

    /// Alternate grid
    alt_grid: Grid<Cell>,

    /// Alt is active
    alt: bool,

    /// The cursor
    cursor: Point,

    /// Alt cursor
    alt_cursor: Point,

    /// Tabstops
    tabs: Vec<bool>,

    /// Mode flags
    mode: TermMode,

    /// Scroll region
    scroll_region: Range<Line>,

    /// Size
    size_info: SizeInfo,

    /// Template cell
    template_cell: Cell,

    /// Empty cell
    empty_cell: Cell,

    pub dirty: bool,
}

/// Terminal size info
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct SizeInfo {
    /// Terminal window width
    pub width: f32,

    /// Terminal window height
    pub height: f32,

    /// Width of individual cell
    pub cell_width: f32,

    /// Height of individual cell
    pub cell_height: f32,
}

impl SizeInfo {
    #[inline]
    pub fn lines(&self) -> Line {
        Line((self.height / self.cell_height) as usize)
    }

    #[inline]
    pub fn cols(&self) -> Column {
        Column((self.width / self.cell_width) as usize)
    }

    pub fn pixels_to_coords(&self, x: usize, y: usize) -> Option<Point> {
        if x > self.width as usize || y > self.height as usize {
            return None;
        }

        let col = Column(x / (self.cell_width as usize));
        let line = Line(y / (self.cell_height as usize));

        Some(Point {
            line: cmp::min(line, self.lines() - 1),
            col: cmp::min(col, self.cols() - 1)
        })
    }
}

impl Term {
    pub fn new(size: SizeInfo) -> Term {
        let template = Cell::default();

        let num_cols = size.cols();
        let num_lines = size.lines();

        let grid = Grid::new(num_lines, num_cols, &template);

        let mut tabs = IndexRange::from(Column(0)..grid.num_cols())
            .map(|i| (*i as usize) % TAB_SPACES == 0)
            .collect::<Vec<bool>>();

        tabs[0] = false;

        let alt = grid.clone();
        let scroll_region = Line(0)..grid.num_lines();

        Term {
            dirty: false,
            grid: grid,
            alt_grid: alt,
            alt: false,
            cursor: Point::default(),
            alt_cursor: Point::default(),
            tabs: tabs,
            mode: Default::default(),
            scroll_region: scroll_region,
            size_info: size,
            template_cell: template.clone(),
            empty_cell: template,
        }
    }

    #[inline]
    pub fn needs_draw(&self) -> bool {
        self.dirty
    }

    pub fn string_from_selection(&self, span: &Span) -> String {
        /// Need a generic push() for the Append trait
        trait PushChar {
            fn push_char(&mut self, c: char);
            fn maybe_newline(&mut self, grid: &Grid<Cell>, line: Line, ending: Column) {
                if ending != Column(0) && !grid[line][ending - 1].flags.contains(cell::WRAPLINE) {
                    self.push_char('\n');
                }
            }
        }

        impl PushChar for String {
            #[inline]
            fn push_char(&mut self, c: char) {
                self.push(c);
            }
        }
        trait Append<T> : PushChar {
            fn append(&mut self, grid: &Grid<Cell>, line: Line, cols: T) -> Option<Range<Column>>;
        }

        use std::ops::{Range, RangeTo, RangeFrom, RangeFull};

        impl Append<Range<Column>> for String {
            fn append(
                &mut self,
                grid: &Grid<Cell>,
                line: Line,
                cols: Range<Column>
            ) -> Option<Range<Column>> {
                let line = &grid[line];
                let line_length = line.line_length();
                let line_end = cmp::min(line_length, cols.end + 1);

                if cols.start >= line_end {
                    None
                } else {
                    for cell in &line[cols.start..line_end] {
                        self.push(cell.c);
                    }

                    Some(cols.start..line_end)
                }
            }
        }

        impl Append<RangeTo<Column>> for String {
            #[inline]
            fn append(&mut self, grid: &Grid<Cell>, line: Line, cols: RangeTo<Column>) -> Option<Range<Column>> {
                self.append(grid, line, Column(0)..cols.end)
            }
        }

        impl Append<RangeFrom<Column>> for String {
            #[inline]
            fn append(
                &mut self,
                grid: &Grid<Cell>,
                line: Line,
                cols: RangeFrom<Column>
            ) -> Option<Range<Column>> {
                let range = self.append(grid, line, cols.start..Column(usize::max_value() - 1));
                range.as_ref()
                    .map(|range| self.maybe_newline(grid, line, range.end));
                range
            }
        }

        impl Append<RangeFull> for String {
            #[inline]
            fn append(
                &mut self,
                grid: &Grid<Cell>,
                line: Line,
                _: RangeFull
            ) -> Option<Range<Column>> {
                let range = self.append(grid, line, Column(0)..Column(usize::max_value() - 1));
                range.as_ref()
                    .map(|range| self.maybe_newline(grid, line, range.end));
                range
            }
        }

        let mut res = String::new();

        let (start, end) = span.to_locations(self.grid.num_cols());
        let line_count = end.line - start.line;

        match line_count {
            // Selection within single line
            Line(0) => {
                res.append(&self.grid, start.line, start.col..end.col);
            },

            // Selection ends on line following start
            Line(1) => {
                // Starting line
                res.append(&self.grid, start.line, start.col..);

                // Ending line
                res.append(&self.grid, end.line, ..end.col);
            },

            // Multi line selection
            _ => {
                // Starting line
                res.append(&self.grid, start.line, start.col..);

                let middle_range = IndexRange::from((start.line + 1)..(end.line));
                for line in middle_range {
                    res.append(&self.grid, line, ..);
                }

                // Ending line
                res.append(&self.grid, end.line, ..end.col);
            }
        }

        res
    }

    /// Convert the given pixel values to a grid coordinate
    ///
    /// The mouse coordinates are expected to be relative to the top left. The
    /// line and column returned are also relative to the top left.
    ///
    /// Returns None if the coordinates are outside the screen
    pub fn pixels_to_coords(&self, x: usize, y: usize) -> Option<Point> {
        self.size_info().pixels_to_coords(x, y)
    }

    /// Access to the raw grid data structure
    ///
    /// This is a bit of a hack; when the window is closed, the event processor
    /// serializes the grid state to a file.
    pub fn grid(&self) -> &Grid<Cell> {
        &self.grid
    }

    /// Iterate over the *renderable* cells in the terminal
    ///
    /// A renderable cell is any cell which has content other than the default
    /// background color.  Cells with an alternate background color are
    /// considered renderable as are cells with any text content.
    pub fn renderable_cells(&mut self, selection: &Selection) -> RenderableCellsIter {
        RenderableCellsIter::new(&mut self.grid, &self.cursor, self.mode, selection)
    }

    /// Resize terminal to new dimensions
    pub fn resize(&mut self, width: f32, height: f32) {
        let size = SizeInfo {
            width: width,
            height: height,
            cell_width: self.size_info.cell_width,
            cell_height: self.size_info.cell_height,
        };

        let old_cols = self.size_info.cols();
        let old_lines = self.size_info.lines();
        let num_cols = size.cols();
        let num_lines = size.lines();

        self.size_info = size;

        if old_cols == num_cols && old_lines == num_lines {
            return;
        }

        // Scroll up to keep cursor and as much context as possible in grid.
        // This only runs when the lines decreases.
        self.scroll_region = Line(0)..self.grid.num_lines();

        // Scroll up to keep cursor in terminal
        if self.cursor.line >= num_lines {
            let lines = self.cursor.line - num_lines + 1;
            self.scroll_up(lines);
            self.cursor.line -= lines;
        }

        println!("num_cols, num_lines = {}, {}", num_cols, num_lines);

        // Resize grids to new size
        let template = self.template_cell.clone();
        self.grid.resize(num_lines, num_cols, &template);
        self.alt_grid.resize(num_lines, num_cols, &template);

        // Ensure cursor is in-bounds
        self.cursor.line = limit(self.cursor.line, Line(0), num_lines);
        self.cursor.col = limit(self.cursor.col, Column(0), num_cols);

        // Recreate tabs list
        self.tabs = IndexRange::from(Column(0)..self.grid.num_cols())
            .map(|i| (*i as usize) % TAB_SPACES == 0)
            .collect::<Vec<bool>>();

        self.tabs[0] = false;

        // Make sure bottom of terminal is clear
        let template = self.empty_cell.clone();
        self.grid.clear_region((self.cursor.line).., |c| c.reset(&template));
        self.alt_grid.clear_region((self.cursor.line).., |c| c.reset(&template));

        // Reset scrolling region to new size
        self.scroll_region = Line(0)..self.grid.num_lines();
    }

    #[inline]
    pub fn size_info(&self) -> &SizeInfo {
        &self.size_info
    }

    #[inline]
    pub fn mode(&self) -> &TermMode {
        &self.mode
    }

    pub fn swap_alt(&mut self) {
        self.alt = !self.alt;
        ::std::mem::swap(&mut self.grid, &mut self.alt_grid);
        ::std::mem::swap(&mut self.cursor, &mut self.alt_cursor);

        if self.alt {
            let template = self.empty_cell.clone();
            self.grid.clear(|c| c.reset(&template));
        }
    }

    /// Scroll screen down
    ///
    /// Text moves down; clear at bottom
    #[inline]
    fn scroll_down_relative(&mut self, origin: Line, lines: Line) {
        debug_println!("scroll_down: {}", lines);

        // Copy of cell template; can't have it borrowed when calling clear/scroll
        let template = self.empty_cell.clone();

        // Clear `lines` lines at bottom of area
        {
            let end = self.scroll_region.end;
            let start = end - lines;
            self.grid.clear_region(start..end, |c| c.reset(&template));
        }

        // Scroll between origin and bottom
        {
            let end = self.scroll_region.end;
            let start = origin + lines;
            self.grid.scroll_down(start..end, lines);
        }
    }

    /// Scroll screen up
    ///
    /// Text moves up; clear at top
    #[inline]
    fn scroll_up_relative(&mut self, origin: Line, lines: Line) {
        debug_println!("scroll_up: {}", lines);

        // Copy of cell template; can't have it borrowed when calling clear/scroll
        let template = self.empty_cell.clone();

        // Clear `lines` lines starting from origin to origin + lines
        {
            let start = origin;
            let end = start + lines;
            self.grid.clear_region(start..end, |c| c.reset(&template));
        }

        // Scroll from origin to bottom less number of lines
        {
            let start = origin;
            let end = self.scroll_region.end - lines;
            self.grid.scroll_up(start..end, lines);
        }
    }
}

impl ansi::TermInfo for Term {
    #[inline]
    fn lines(&self) -> Line {
        self.grid.num_lines()
    }

    #[inline]
    fn cols(&self) -> Column {
        self.grid.num_cols()
    }
}

impl ansi::Handler for Term {
    /// A character to be displayed
    #[inline]
    fn input(&mut self, c: char) {
        if self.cursor.col == self.grid.num_cols() {
            debug_println!("wrapping");
            {
                let location = Point {
                    line: self.cursor.line,
                    col: self.cursor.col - 1
                };

                let cell = &mut self.grid[&location];
                cell.flags.insert(cell::WRAPLINE);
            }
            if (self.cursor.line + 1) >= self.scroll_region.end {
                self.linefeed();
            } else {
                self.cursor.line += 1;
            }
            self.cursor.col = Column(0);
        }

        unsafe {
            if ::std::intrinsics::unlikely(self.cursor.line == self.grid.num_lines()) {
                panic!("cursor fell off grid");
            }
        }

        let cell = &mut self.grid[&self.cursor];
        *cell = self.template_cell.clone();
        cell.c = c;
        self.cursor.col += 1;
    }

    #[inline]
    fn goto(&mut self, line: Line, col: Column) {
        use std::cmp::min;
        debug_println!("goto: line={}, col={}", line, col);
        self.cursor.line = min(line, self.grid.num_lines() - 1);
        self.cursor.col = min(col, self.grid.num_cols() - 1);
    }

    #[inline]
    fn goto_line(&mut self, line: Line) {
        debug_println!("goto_line: {}", line);
        self.cursor.line = line;
    }

    #[inline]
    fn goto_col(&mut self, col: Column) {
        debug_println!("goto_col: {}", col);
        self.cursor.col = col;
    }

    #[inline]
    fn insert_blank(&mut self, count: Column) {
        // Ensure inserting within terminal bounds
        let count = ::std::cmp::min(count, self.size_info.cols() - self.cursor.col);

        let source = self.cursor.col;
        let destination = self.cursor.col + count;
        let num_cells = (self.size_info.cols() - destination).0;

        let line = self.cursor.line; // borrowck
        let line = &mut self.grid[line];

        unsafe {
            let src = line[source..].as_ptr();
            let dst = line[destination..].as_mut_ptr();

            ptr::copy(src, dst, num_cells);
        }

        // Cells were just moved out towards the end of the line; fill in
        // between source and dest with blanks.
        let template = self.empty_cell.clone();
        for c in &mut line[source..destination] {
            c.reset(&template);
        }
    }

    #[inline]
    fn move_up(&mut self, lines: Line) {
        debug_println!("move_up: {}", lines);
        self.cursor.line -= lines;
    }

    #[inline]
    fn move_down(&mut self, lines: Line) {
        debug_println!("move_down: {}", lines);
        self.cursor.line += lines;
    }

    #[inline]
    fn move_forward(&mut self, cols: Column) {
        debug_println!("move_forward: {}", cols);
        self.cursor.col += cols;
    }

    #[inline]
    fn move_backward(&mut self, cols: Column) {
        debug_println!("move_backward: {}", cols);
        if cols > self.cursor.col {
            self.cursor.col = Column(0);
        } else {
            self.cursor.col -= cols;
        }
    }

    #[inline]
    fn identify_terminal<W: io::Write>(&mut self, writer: &mut W) {
        let _ = writer.write_all(b"\x1b[?6c");
    }

    #[inline]
    fn move_down_and_cr(&mut self, lines: Line) {
        err_println!("[unimplemented] move_down_and_cr: {}", lines);
    }

    #[inline]
    fn move_up_and_cr(&mut self, lines: Line) {
        err_println!("[unimplemented] move_up_and_cr: {}", lines);
    }

    #[inline]
    fn put_tab(&mut self, mut count: i64) {
        debug_println!("put_tab: {}", count);

        let mut col = self.cursor.col;
        while col < self.grid.num_cols() && count != 0 {
            count -= 1;
            loop {
                if col == self.grid.num_cols() || self.tabs[*col as usize] {
                    break;
                }
                col += 1;
            }
        }

        self.cursor.col = col;
    }

    /// Backspace `count` characters
    #[inline]
    fn backspace(&mut self) {
        debug_println!("backspace");
        if self.cursor.col > Column(0) {
            self.cursor.col -= 1;
        }
    }

    /// Carriage return
    #[inline]
    fn carriage_return(&mut self) {
        debug_println!("carriage_return");
        self.cursor.col = Column(0);
    }

    /// Linefeed
    #[inline]
    fn linefeed(&mut self) {
        debug_println!("linefeed");
        if self.cursor.line + 1 == self.scroll_region.end {
            self.scroll_up(Line(1));
        } else {
            self.cursor.line += 1;
        }
    }

    /// Set current position as a tabstop
    #[inline]
    fn bell(&mut self) {
        debug_println!("bell");
    }

    #[inline]
    fn substitute(&mut self) {
        err_println!("[unimplemented] substitute");
    }

    #[inline]
    fn newline(&mut self) {
        err_println!("[unimplemented] newline");
    }

    #[inline]
    fn set_horizontal_tabstop(&mut self) {
        err_println!("[unimplemented] set_horizontal_tabstop");
    }

    #[inline]
    fn scroll_up(&mut self, lines: Line) {
        let origin = self.scroll_region.start;
        self.scroll_up_relative(origin, lines);
    }

    #[inline]
    fn scroll_down(&mut self, lines: Line) {
        let origin = self.scroll_region.start;
        self.scroll_down_relative(origin, lines);
    }

    #[inline]
    fn insert_blank_lines(&mut self, lines: Line) {
        debug_println!("insert_blank_lines: {}", lines);
        if self.scroll_region.contains_(self.cursor.line) {
            let origin = self.cursor.line;
            self.scroll_down_relative(origin, lines);
        }
    }

    #[inline]
    fn delete_lines(&mut self, lines: Line) {
        debug_println!("delete_lines: {}", lines);
        if self.scroll_region.contains_(self.cursor.line) {
            let origin = self.cursor.line;
            self.scroll_up_relative(origin, lines);
        }
    }

    #[inline]
    fn erase_chars(&mut self, count: Column) {
        debug_println!("erase_chars: {}", count);
        let start = self.cursor.col;
        let end = start + count;

        let row = &mut self.grid[self.cursor.line];
        let template = self.empty_cell.clone();
        for c in &mut row[start..end] {
            c.reset(&template);
        }
    }

    #[inline]
    fn delete_chars(&mut self, count: Column) {
        // Ensure deleting within terminal bounds
        let count = ::std::cmp::min(count, self.size_info.cols());

        let start = self.cursor.col;
        let end = self.cursor.col + count;
        let n = (self.size_info.cols() - end).0;

        let line = self.cursor.line; // borrowck
        let line = &mut self.grid[line];

        unsafe {
            let src = line[end..].as_ptr();
            let dst = line[start..].as_mut_ptr();

            ptr::copy(src, dst, n);
        }

        // Clear last `count` cells in line. If deleting 1 char, need to delete
        // 1 cell.
        let template = self.empty_cell.clone();
        let end = self.size_info.cols() - count;
        for c in &mut line[end..] {
            c.reset(&template);
        }
    }

    #[inline]
    fn move_backward_tabs(&mut self, count: i64) {
        err_println!("[unimplemented] move_backward_tabs: {}", count);
    }

    #[inline]
    fn move_forward_tabs(&mut self, count: i64) {
        err_println!("[unimplemented] move_forward_tabs: {}", count);
    }

    #[inline]
    fn save_cursor_position(&mut self) {
        err_println!("[unimplemented] save_cursor_position");
    }

    #[inline]
    fn restore_cursor_position(&mut self) {
        err_println!("[unimplemented] restore_cursor_position");
    }

    #[inline]
    fn clear_line(&mut self, mode: ansi::LineClearMode) {
        debug_println!("clear_line: {:?}", mode);
        let template = self.empty_cell.clone();
        match mode {
            ansi::LineClearMode::Right => {
                let row = &mut self.grid[self.cursor.line];
                for cell in &mut row[self.cursor.col..] {
                    cell.reset(&template);
                }
            },
            ansi::LineClearMode::Left => {
                let row = &mut self.grid[self.cursor.line];
                for cell in &mut row[..(self.cursor.col + 1)] {
                    cell.reset(&template);
                }
            },
            ansi::LineClearMode::All => {
                let row = &mut self.grid[self.cursor.line];
                for cell in &mut row[..] {
                    cell.reset(&template);
                }
            },
        }
    }

    #[inline]
    fn clear_screen(&mut self, mode: ansi::ClearMode) {
        debug_println!("clear_screen: {:?}", mode);
        let template = self.empty_cell.clone();
        match mode {
            ansi::ClearMode::Below => {
                for row in &mut self.grid[self.cursor.line..] {
                    for cell in row {
                        cell.reset(&template);
                    }
                }
            },
            ansi::ClearMode::All => {
                self.grid.clear(|c| c.reset(&template));
            },
            _ => {
                panic!("ansi::ClearMode::Above not implemented");
            }
        }
    }

    #[inline]
    fn clear_tabs(&mut self, mode: ansi::TabulationClearMode) {
        err_println!("[unimplemented] clear_tabs: {:?}", mode);
    }

    #[inline]
    fn reset_state(&mut self) {
        err_println!("[unimplemented] reset_state");
    }

    #[inline]
    fn reverse_index(&mut self) {
        debug_println!("reverse_index");
        // if cursor is at the top
        if self.cursor.line == self.scroll_region.start {
            self.scroll_down(Line(1));
        } else {
            self.cursor.line -= 1;
        }
    }

    /// set a terminal attribute
    #[inline]
    fn terminal_attribute(&mut self, attr: Attr) {
        debug_println!("Set Attribute: {:?}", attr);
        match attr {
            Attr::Foreground(color) => self.template_cell.fg = color,
            Attr::Background(color) => self.template_cell.bg = color,
            Attr::Reset => {
                self.template_cell.fg = Color::Named(NamedColor::Foreground);
                self.template_cell.bg = Color::Named(NamedColor::Background);
                self.template_cell.flags = cell::Flags::empty();
            },
            Attr::Reverse => self.template_cell.flags.insert(cell::INVERSE),
            Attr::CancelReverse => self.template_cell.flags.remove(cell::INVERSE),
            Attr::Bold => self.template_cell.flags.insert(cell::BOLD),
            Attr::CancelBoldDim => self.template_cell.flags.remove(cell::BOLD),
            Attr::Italic => self.template_cell.flags.insert(cell::ITALIC),
            Attr::CancelItalic => self.template_cell.flags.remove(cell::ITALIC),
            Attr::Underscore => self.template_cell.flags.insert(cell::UNDERLINE),
            Attr::CancelUnderline => self.template_cell.flags.remove(cell::UNDERLINE),
            _ => {
                debug_println!("Term got unhandled attr: {:?}", attr);
            }
        }
    }

    #[inline]
    fn set_mode(&mut self, mode: ansi::Mode) {
        debug_println!("set_mode: {:?}", mode);
        match mode {
            ansi::Mode::SwapScreenAndSetRestoreCursor => self.swap_alt(),
            ansi::Mode::ShowCursor => self.mode.insert(mode::SHOW_CURSOR),
            ansi::Mode::CursorKeys => self.mode.insert(mode::APP_CURSOR),
            ansi::Mode::ReportMouseClicks => self.mode.insert(mode::MOUSE_REPORT_CLICK),
            ansi::Mode::ReportMouseMotion => self.mode.insert(mode::MOUSE_MOTION),
            ansi::Mode::BracketedPaste => self.mode.insert(mode::BRACKETED_PASTE),
            ansi::Mode::SgrMouse => self.mode.insert(mode::SGR_MOUSE),
            _ => {
                debug_println!(".. ignoring set_mode");
            }
        }
    }

    #[inline]
    fn unset_mode(&mut self,mode: ansi::Mode) {
        debug_println!("unset_mode: {:?}", mode);
        match mode {
            ansi::Mode::SwapScreenAndSetRestoreCursor => self.swap_alt(),
            ansi::Mode::ShowCursor => self.mode.remove(mode::SHOW_CURSOR),
            ansi::Mode::CursorKeys => self.mode.remove(mode::APP_CURSOR),
            ansi::Mode::ReportMouseClicks => self.mode.remove(mode::MOUSE_REPORT_CLICK),
            ansi::Mode::ReportMouseMotion => self.mode.remove(mode::MOUSE_MOTION),
            ansi::Mode::BracketedPaste => self.mode.remove(mode::BRACKETED_PASTE),
            ansi::Mode::SgrMouse => self.mode.remove(mode::SGR_MOUSE),
            _ => {
                debug_println!(".. ignoring unset_mode");
            }
        }
    }

    #[inline]
    fn set_scrolling_region(&mut self, region: Range<Line>) {
        debug_println!("set scroll region: {:?}", region);
        self.scroll_region = region;
        self.goto(Line(0), Column(0));
    }

    #[inline]
    fn set_keypad_application_mode(&mut self) {
        debug_println!("set mode::APP_KEYPAD");
        self.mode.insert(mode::APP_KEYPAD);
    }

    #[inline]
    fn unset_keypad_application_mode(&mut self) {
        debug_println!("unset mode::APP_KEYPAD");
        self.mode.remove(mode::APP_KEYPAD);
    }
}

#[cfg(test)]
mod tests {
    extern crate serde_json;
    extern crate test;

    use super::limit;

    use grid::Grid;
    use index::{Line, Column};
    use term::{Cell};

    /// Check that the grid can be serialized back and forth losslessly
    ///
    /// This test is in the term module as opposed to the grid since we want to
    /// test this property with a T=Cell.
    #[test]
    fn grid_serde() {
        let template = Cell::default();

        let grid = Grid::new(Line(24), Column(80), &template);
        let serialized = serde_json::to_string(&grid).expect("ser");
        let deserialized = serde_json::from_str::<Grid<Cell>>(&serialized)
                                      .expect("de");

        assert_eq!(deserialized, grid);
    }

    #[test]
    fn limit_works() {
        assert_eq!(limit(5, 1, 10), 5);
        assert_eq!(limit(5, 6, 10), 6);
        assert_eq!(limit(5, 1, 4), 4);
    }
}

#[cfg(test)]
mod benches {
    extern crate test;
    extern crate serde_json as json;

    use std::io::Read;
    use std::fs::File;
    use std::mem;
    use std::path::Path;

    use grid::Grid;
    use selection::Selection;

    use super::{SizeInfo, Term};
    use super::cell::Cell;

    fn read_string<P>(path: P) -> String
        where P: AsRef<Path>
    {
        let mut res = String::new();
        File::open(path.as_ref()).unwrap()
            .read_to_string(&mut res).unwrap();

        res
    }

    /// Benchmark for the renderable cells iterator
    ///
    /// The renderable cells iterator yields cells that require work to be
    /// displayed (that is, not a an empty background cell). This benchmark
    /// measures how long it takes to process the whole iterator.
    ///
    /// When this benchmark was first added, it averaged ~78usec on my macbook
    /// pro. The total render time for this grid is anywhere between ~1500 and
    /// ~2000usec (measured imprecisely with the visual meter).
    #[bench]
    fn render_iter(b: &mut test::Bencher) {
        // Need some realistic grid state; using one of the ref files.
        let serialized_grid = read_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/ref/vim_large_window_scroll/grid.json")
        );
        let serialized_size = read_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/ref/vim_large_window_scroll/size.json")
        );

        let mut grid: Grid<Cell> = json::from_str(&serialized_grid).unwrap();
        let size: SizeInfo = json::from_str(&serialized_size).unwrap();

        let mut terminal = Term::new(size);
        mem::swap(&mut terminal.grid, &mut grid);

        b.iter(|| {
            let iter = terminal.renderable_cells(&Selection::Empty);
            for cell in iter {
                test::black_box(cell);
            }
        })
    }
}
