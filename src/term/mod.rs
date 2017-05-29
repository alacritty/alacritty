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
use std::mem;
use std::ops::{Range, Index, IndexMut};
use std::ptr;
use std::cmp::{min, max};
use std::io;
use std::time::{Duration, Instant};

use unicode_width::UnicodeWidthChar;

use ansi::{self, Color, NamedColor, Attr, Handler, CharsetIndex, StandardCharset};
use grid::{BidirectionalIterator, Grid, ClearRegion, ToRange, Indexed};
use index::{self, Point, Column, Line, Linear, IndexRange, Contains, RangeInclusive, Side};
use selection::{Span, Selection};
use config::{Config, VisualBellAnimation};
use Rgb;

pub mod cell;
pub mod color;
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
    config: &'a Config,
    colors: &'a color::List,
    selection: Option<RangeInclusive<index::Linear>>,
    cursor_original: (Option<Indexed<Cell>>, Option<Indexed<Cell>>),
}

impl<'a> RenderableCellsIter<'a> {
    /// Create the renderable cells iterator
    ///
    /// The cursor and terminal mode are required for properly displaying the
    /// cursor.
    fn new<'b>(
        grid: &'b mut Grid<Cell>,
        cursor: &'b Point,
        colors: &'b color::List,
        mode: TermMode,
        config: &'b Config,
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
            config: config,
            colors: colors,
            cursor_original: (None, None),
        }.initialize()
    }

    fn initialize(mut self) -> Self {
        if self.cursor_is_visible() {
            self.cursor_original.0 = Some(Indexed {
                line:   self.cursor.line,
                column: self.cursor.col,
                inner:  self.grid[self.cursor]
            });
            let mut spacer = false;
            let mut location = *self.cursor;

            if self.grid[self.cursor].flags.contains(cell::WIDE_CHAR) &&
                self.cursor.col + 1 < self.grid.num_cols()
            {
                spacer = true;
                location.col += 1;
                self.cursor_original.1 = Some(Indexed {
                    line:   location.line,
                    column: location.col,
                    inner:  self.grid[&location]
                });
            }

            if self.config.custom_cursor_colors() {
                {
                    let cell = &mut self.grid[self.cursor];
                    cell.fg = Color::Named(NamedColor::CursorText);
                    cell.bg = Color::Named(NamedColor::Cursor);
                }
                if spacer {
                    let cell = &mut self.grid[&location];
                    cell.fg = Color::Named(NamedColor::CursorText);
                    cell.bg = Color::Named(NamedColor::Cursor);
                }
            } else {
                {
                    let cell = &mut self.grid[self.cursor];
                    mem::swap(&mut cell.fg, &mut cell.bg);
                }
                if spacer {
                    let cell = &mut self.grid[&location];
                    mem::swap(&mut cell.fg, &mut cell.bg);
                }
            }
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
            if let Some(ref original) = self.cursor_original.0 {
                self.grid[self.cursor] = original.inner;
            }
            if let Some(ref original) = self.cursor_original.1 {
                let mut location = *self.cursor;
                location.col += 1;
                self.grid[&location] = original.inner;
            }
        }
    }
}

pub struct RenderableCell {
    pub line: Line,
    pub column: Column,
    pub c: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub flags: cell::Flags,
}

impl<'a> Iterator for RenderableCellsIter<'a> {
    type Item = RenderableCell;

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

                // Get Rgb value for foreground
                let fg = match *fg {
                    Color::Spec(rgb) => rgb,
                    Color::Named(ansi) => {
                        if self.config.draw_bold_text_with_bright_colors() && cell.bold() {
                            self.colors[ansi.to_bright()]
                        } else {
                            self.colors[ansi]
                        }
                    },
                    Color::Indexed(idx) => {
                        let idx = if self.config.draw_bold_text_with_bright_colors()
                            && cell.bold()
                            && idx < 8
                        {
                            idx + 8
                        } else {
                            idx
                        };

                        self.colors[idx]
                    }
                };

                // Get Rgb value for background
                let bg = match *bg {
                    Color::Spec(rgb) => rgb,
                    Color::Named(ansi) => self.colors[ansi],
                    Color::Indexed(idx) => self.colors[idx],
                };

                return Some(RenderableCell {
                    line: line,
                    column: column,
                    flags: cell.flags,
                    c: cell.c,
                    fg: fg,
                    bg: bg,
                })
            }

            self.column = Column(0);
            self.line += 1;
        }

        None
    }
}

pub mod mode {
    bitflags! {
        pub flags TermMode: u16 {
            const SHOW_CURSOR         = 0b000000000001,
            const APP_CURSOR          = 0b000000000010,
            const APP_KEYPAD          = 0b000000000100,
            const MOUSE_REPORT_CLICK  = 0b000000001000,
            const BRACKETED_PASTE     = 0b000000010000,
            const SGR_MOUSE           = 0b000000100000,
            const MOUSE_MOTION        = 0b000001000000,
            const LINE_WRAP           = 0b000010000000,
            const LINE_FEED_NEW_LINE  = 0b000100000000,
            const ORIGIN              = 0b001000000000,
            const INSERT              = 0b010000000000,
            const FOCUS_IN_OUT        = 0b100000000000,
            const ANY                 = 0b111111111111,
            const NONE                = 0,
        }
    }

    impl Default for TermMode {
        fn default() -> TermMode {
            SHOW_CURSOR | LINE_WRAP
        }
    }
}

pub use self::mode::TermMode;

pub const TAB_SPACES: usize = 8;

trait CharsetMapping {
    fn map(&self, c: char) -> char {
        c
    }
}

impl CharsetMapping for StandardCharset {
    /// Switch/Map character to the active charset. Ascii is the common case and
    /// for that we want to do as little as possible.
    #[inline]
    fn map(&self, c: char) -> char {
        match *self {
            StandardCharset::Ascii => c,
            StandardCharset::SpecialCharacterAndLineDrawing =>
                match c {
                    '`' => '◆',
                    'a' => '▒',
                    'b' => '\t',
                    'c' => '\u{000c}',
                    'd' => '\r',
                    'e' => '\n',
                    'f' => '°',
                    'g' => '±',
                    'h' => '\u{2424}',
                    'i' => '\u{000b}',
                    'j' => '┘',
                    'k' => '┐',
                    'l' => '┌',
                    'm' => '└',
                    'n' => '┼',
                    'o' => '⎺',
                    'p' => '⎻',
                    'q' => '─',
                    'r' => '⎼',
                    's' => '⎽',
                    't' => '├',
                    'u' => '┤',
                    'v' => '┴',
                    'w' => '┬',
                    'x' => '│',
                    'y' => '≤',
                    'z' => '≥',
                    '{' => 'π',
                    '|' => '≠',
                    '}' => '£',
                    '~' => '·',
                    _ => c
                },
        }
    }
}

#[derive(Default, Copy, Clone)]
struct Charsets([StandardCharset; 4]);

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

#[derive(Default, Copy, Clone)]
pub struct Cursor {
    /// The location of this cursor
    point: Point,

    /// Template cell when using this cursor
    template: Cell,

    /// Currently configured graphic character sets
    charsets: Charsets,
}

pub struct VisualBell {
    /// Visual bell animation
    animation: VisualBellAnimation,

    /// Visual bell duration
    duration: Duration,

    /// The last time the visual bell rang, if at all
    start_time: Option<Instant>,
}

fn cubic_bezier(p0: f64, p1: f64, p2: f64, p3: f64, x: f64) -> f64 {
    (1.0 - x).powi(3) * p0 +
    3.0 * (1.0 - x).powi(2) * x * p1 +
    3.0 * (1.0 - x) * x.powi(2) * p2 +
    x.powi(3) * p3
}

impl VisualBell {
    pub fn new(config: &Config) -> VisualBell {
        let visual_bell_config = config.visual_bell();
        VisualBell {
            animation: visual_bell_config.animation(),
            duration: visual_bell_config.duration(),
            start_time: None,
        }
    }

    /// Ring the visual bell, and return its intensity.
    pub fn ring(&mut self) -> f64 {
        let now = Instant::now();
        self.start_time = Some(now);
        self.intensity_at_instant(now)
    }

    /// Get the currenty intensity of the visual bell. The bell's intensity
    /// ramps down from 1.0 to 0.0 at a rate determined by the bell's duration.
    pub fn intensity(&self) -> f64 {
        self.intensity_at_instant(Instant::now())
    }

    /// Check whether or not the visual bell has completed "ringing".
    pub fn completed(&mut self) -> bool {
        match self.start_time {
            Some(earlier) => {
                if Instant::now().duration_since(earlier) >= self.duration {
                    self.start_time = None;
                }
                false
            },
            None => true
        }
    }

    /// Get the intensity of the visual bell at a particular instant. The bell's
    /// intensity ramps down from 1.0 to 0.0 at a rate determined by the bell's
    /// duration.
    pub fn intensity_at_instant(&self, instant: Instant) -> f64 {
        // If `duration` is zero, then the VisualBell is disabled; therefore,
        // its `intensity` is zero.
        if self.duration == Duration::from_secs(0) {
            return 0.0;
        }

        match self.start_time {
            // Similarly, if `start_time` is `None`, then the VisualBell has not
            // been "rung"; therefore, its `intensity` is zero.
            None => 0.0,

            Some(earlier) => {
                // Finally, if the `instant` at which we wish to compute the
                // VisualBell's `intensity` occurred before the VisualBell was
                // "rung", then its `intensity` is also zero.
                if instant < earlier {
                    return 0.0;
                }

                let elapsed = instant.duration_since(earlier);
                let elapsed_f = elapsed.as_secs() as f64 +
                                elapsed.subsec_nanos() as f64 / 1e9f64;
                let duration_f = self.duration.as_secs() as f64 +
                                 self.duration.subsec_nanos() as f64 / 1e9f64;

                // Otherwise, we compute a value `time` from 0.0 to 1.0
                // inclusive that represents the ratio of `elapsed` time to the
                // `duration` of the VisualBell.
                let time = (elapsed_f / duration_f).min(1.0);

                // We use this to compute the inverse `intensity` of the
                // VisualBell. When `time` is 0.0, `inverse_intensity` is 0.0,
                // and when `time` is 1.0, `inverse_intensity` is 1.0.
                let inverse_intensity = match self.animation {
                    VisualBellAnimation::Ease => cubic_bezier(0.25, 0.1, 0.25, 1.0, time),
                    VisualBellAnimation::EaseOut => cubic_bezier(0.25, 0.1, 0.25, 1.0, time),
                    VisualBellAnimation::EaseOutSine => cubic_bezier(0.39, 0.575, 0.565, 1.0, time),
                    VisualBellAnimation::EaseOutQuad => cubic_bezier(0.25, 0.46, 0.45, 0.94, time),
                    VisualBellAnimation::EaseOutCubic => cubic_bezier(0.215, 0.61, 0.355, 1.0, time),
                    VisualBellAnimation::EaseOutQuart => cubic_bezier(0.165, 0.84, 0.44, 1.0, time),
                    VisualBellAnimation::EaseOutQuint => cubic_bezier(0.23, 1.0, 0.32, 1.0, time),
                    VisualBellAnimation::EaseOutExpo => cubic_bezier(0.19, 1.0, 0.22, 1.0, time),
                    VisualBellAnimation::EaseOutCirc => cubic_bezier(0.075, 0.82, 0.165, 1.0, time),
                    VisualBellAnimation::Linear => time,
                };

                // Since we want the `intensity` of the VisualBell to decay over
                // `time`, we subtract the `inverse_intensity` from 1.0.
                1.0 - inverse_intensity
            }
        }
    }

    pub fn update_config(&mut self, config: &Config) {
        let visual_bell_config = config.visual_bell();
        self.animation = visual_bell_config.animation();
        self.duration = visual_bell_config.duration();
    }
}

pub struct Term {
    /// The grid
    grid: Grid<Cell>,

    /// Tracks if the next call to input will need to first handle wrapping.
    /// This is true after the last column is set with the input function. Any function that
    /// implicitly sets the line or column needs to set this to false to avoid wrapping twice.
    /// input_needs_wrap ensures that cursor.col is always valid for use into indexing into
    /// arrays. Without it we wold have to sanitize cursor.col every time we used it.
    input_needs_wrap: bool,

    /// Got a request to set title; it's buffered here until next draw.
    ///
    /// Would be nice to avoid the allocation...
    next_title: Option<String>,

    /// Alternate grid
    alt_grid: Grid<Cell>,

    /// Alt is active
    alt: bool,

    /// The cursor
    cursor: Cursor,

    /// The graphic character set, out of `charsets`, which ASCII is currently
    /// being mapped to
    active_charset: CharsetIndex,

    /// Tabstops
    tabs: Vec<bool>,

    /// Mode flags
    mode: TermMode,

    /// Scroll region
    scroll_region: Range<Line>,

    /// Size
    size_info: SizeInfo,

    /// Empty cell
    empty_cell: Cell,

    pub dirty: bool,

    pub visual_bell: VisualBell,

    /// Saved cursor from main grid
    cursor_save: Cursor,

    /// Saved cursor from alt grid
    cursor_save_alt: Cursor,

    semantic_escape_chars: String,

    /// Colors used for rendering
    colors: color::List,
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

    /// Horizontal window padding
    pub padding_x: f32,

    /// Horizontal window padding
    pub padding_y: f32,
}

impl SizeInfo {
    #[inline]
    pub fn lines(&self) -> Line {
        Line(((self.height - 2. * self.padding_y) / self.cell_height) as usize)
    }

    #[inline]
    pub fn cols(&self) -> Column {
        Column(((self.width - 2. * self.padding_x) / self.cell_width) as usize)
    }

    fn contains_point(&self, x: usize, y:usize) -> bool {
        x <= (self.width - self.padding_x) as usize &&
            x >= self.padding_x as usize &&
            y <= (self.height - self.padding_y) as usize &&
            y >= self.padding_y as usize
    }

    pub fn pixels_to_coords(&self, x: usize, y: usize) -> Option<Point> {
        if !self.contains_point(x, y) {
            return None;
        }

        let col = Column((x - self.padding_x as usize) / (self.cell_width as usize));
        let line = Line((y - self.padding_y as usize) / (self.cell_height as usize));

        Some(Point {
            line: min(line, self.lines() - 1),
            col: min(col, self.cols() - 1)
        })
    }
}


impl Term {
    #[inline]
    pub fn get_next_title(&mut self) -> Option<String> {
        self.next_title.take()
    }

    pub fn new(config : &Config, size: SizeInfo) -> Term {
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
            next_title: None,
            dirty: false,
            visual_bell: VisualBell::new(config),
            input_needs_wrap: false,
            grid: grid,
            alt_grid: alt,
            alt: false,
            active_charset: Default::default(),
            cursor: Default::default(),
            cursor_save: Default::default(),
            cursor_save_alt: Default::default(),
            tabs: tabs,
            mode: Default::default(),
            scroll_region: scroll_region,
            size_info: size,
            empty_cell: template,
            colors: color::List::from(config.colors()),
            semantic_escape_chars: config.selection().semantic_escape_chars.clone(),
        }
    }

    pub fn update_config(&mut self, config: &Config) {
        self.semantic_escape_chars = config.selection().semantic_escape_chars.clone();
        self.colors.fill_named(config.colors());
        self.visual_bell.update_config(config);
    }

    #[inline]
    pub fn needs_draw(&self) -> bool {
        self.dirty
    }

    pub fn line_selection(&self, selection: &mut Selection, point: Point) {
        selection.clear();
        selection.update(Point {
            line: point.line,
            col: Column(0),
        }, Side::Left);
        selection.update(Point {
            line: point.line,
            col: self.grid.num_cols() - Column(1),
        }, Side::Right);
    }

    pub fn semantic_selection(&self, selection: &mut Selection, point: Point) {
        let mut side_left = Point {
            line: point.line,
            col: point.col
        };
        let mut side_right = Point {
            line: point.line,
            col: point.col
        };

        let mut left_iter = self.grid.iter_from(point);
        let mut right_iter = self.grid.iter_from(point);

        let last_col = self.grid.num_cols() - Column(1);

        while let Some(cell) = left_iter.prev() {
            if self.semantic_escape_chars.contains(cell.c) {
                break;
            }

            if left_iter.cur.col == last_col && !cell.flags.contains(cell::WRAPLINE) {
                break; // cut off if on new line or hit escape char
            }

            side_left.col = left_iter.cur.col;
            side_left.line = left_iter.cur.line;
        }

        while let Some(cell) = right_iter.next() {
            if self.semantic_escape_chars.contains(cell.c) {
                break;
            }

            side_right.col = right_iter.cur.col;
            side_right.line = right_iter.cur.line;

            if right_iter.cur.col == last_col && !cell.flags.contains(cell::WRAPLINE) {
                break; // cut off if on new line or hit escape char
            }
        }

        selection.clear();
        selection.update(side_left, Side::Left);
        selection.update(side_right, Side::Right);
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
                let line_end = min(line_length, cols.end + 1);

                if cols.start >= line_end {
                    None
                } else {
                    for cell in &line[cols.start..line_end] {
                        if !cell.flags.contains(cell::WIDE_CHAR_SPACER) {
                            self.push(cell.c);
                        }
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
    pub fn renderable_cells<'b>(
        &'b mut self,
        config: &'b Config,
        selection: &'b Selection
    ) -> RenderableCellsIter {
        RenderableCellsIter::new(
            &mut self.grid,
            &self.cursor.point,
            &self.colors,
            self.mode,
            config,
            selection,
        )
    }

    /// Resize terminal to new dimensions
    pub fn resize(&mut self, width: f32, height: f32) {
        debug!("Term::resize");
        // Bounds check; lots of math assumes width and height are > 0
        if width as usize <= 2 * self.size_info.padding_x as usize ||
            height as usize <= 2 * self.size_info.padding_y as usize
        {
            return;
        }

        let size = SizeInfo {
            width: width,
            height: height,
            cell_width: self.size_info.cell_width,
            cell_height: self.size_info.cell_height,
            padding_x: self.size_info.padding_x,
            padding_y: self.size_info.padding_y,
        };

        let old_cols = self.grid.num_cols();
        let old_lines = self.grid.num_lines();
        let mut num_cols = size.cols();
        let mut num_lines = size.lines();

        self.size_info = size;

        if old_cols == num_cols && old_lines == num_lines {
            debug!("Term::resize dimensions unchanged");
            return;
        }

        // Should not allow less than 1 col, causes all sorts of checks to be required.
        if num_cols <= Column(1) {
            num_cols = Column(2);
        }

        // Should not allow less than 1 line, causes all sorts of checks to be required.
        if num_lines <= Line(1) {
            num_lines = Line(2);
        }

        // Scroll up to keep cursor and as much context as possible in grid.
        // This only runs when the lines decreases.
        self.scroll_region = Line(0)..self.grid.num_lines();

        // Scroll up to keep cursor in terminal
        if self.cursor.point.line >= num_lines {
            let lines = self.cursor.point.line - num_lines + 1;
            self.grid.scroll_up(Line(0)..old_lines, lines);
        }

        // Scroll up alt grid as well
        if self.cursor_save_alt.point.line >= num_lines {
            let lines = self.cursor_save_alt.point.line - num_lines + 1;
            self.alt_grid.scroll_up(Line(0)..old_lines, lines);
        }

        debug!("num_cols, num_lines = {}, {}", num_cols, num_lines);

        // Resize grids to new size
        let template = self.empty_cell;
        self.grid.resize(num_lines, num_cols, &template);
        self.alt_grid.resize(num_lines, num_cols, &template);

        // Reset scrolling region to new size
        self.scroll_region = Line(0)..self.grid.num_lines();

        // Ensure cursors are in-bounds.
        self.cursor.point.col = min(self.cursor.point.col, num_cols - 1);
        self.cursor.point.line = min(self.cursor.point.line, num_lines - 1);
        self.cursor_save.point.col = min(self.cursor_save.point.col, num_cols - 1);
        self.cursor_save.point.line = min(self.cursor_save.point.line, num_lines - 1);
        self.cursor_save_alt.point.col = min(self.cursor_save_alt.point.col, num_cols - 1);
        self.cursor_save_alt.point.line = min(self.cursor_save_alt.point.line, num_lines - 1);

        // Recreate tabs list
        self.tabs = IndexRange::from(Column(0)..self.grid.num_cols())
            .map(|i| (*i as usize) % TAB_SPACES == 0)
            .collect::<Vec<bool>>();

        self.tabs[0] = false;

        if num_lines > old_lines {
            // Make sure bottom of terminal is clear
            let template = self.empty_cell;
            self.grid.clear_region((self.cursor.point.line + 1).., |c| c.reset(&template));
            self.alt_grid.clear_region((self.cursor_save_alt.point.line + 1).., |c| c.reset(&template));
        }

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
        if self.alt {
            let template = self.empty_cell;
            self.grid.clear(|c| c.reset(&template));
        }

        self.alt = !self.alt;
        ::std::mem::swap(&mut self.grid, &mut self.alt_grid);
    }

    /// Scroll screen down
    ///
    /// Text moves down; clear at bottom
    /// Expects origin to be in scroll range.
    #[inline]
    fn scroll_down_relative(&mut self, origin: Line, lines: Line) {
        trace!("scroll_down_relative: origin={}, lines={}", origin, lines);
        let lines = min(lines, self.scroll_region.end - self.scroll_region.start);

        // Copy of cell template; can't have it borrowed when calling clear/scroll
        let template = self.empty_cell;

        // Clear `lines` lines at bottom of area
        {
            let start = max(origin, Line(self.scroll_region.end.0.saturating_sub(lines.0)));
            self.grid.clear_region(start..self.scroll_region.end, |c| c.reset(&template));
        }

        // Scroll between origin and bottom
        self.grid.scroll_down(origin..self.scroll_region.end, lines);
    }

    /// Scroll screen up
    ///
    /// Text moves up; clear at top
    /// Expects origin to be in scroll range.
    #[inline]
    fn scroll_up_relative(&mut self, origin: Line, lines: Line) {
        trace!("scroll_up_relative: origin={}, lines={}", origin, lines);
        let lines = min(lines, self.scroll_region.end - self.scroll_region.start);

        // Copy of cell template; can't have it borrowed when calling clear/scroll
        let template = self.empty_cell;

        // Clear `lines` lines starting from origin to origin + lines
        {
            let end = min(origin + lines, self.scroll_region.end);
            self.grid.clear_region(origin..end, |c| c.reset(&template));
        }

        // Scroll from origin to bottom less number of lines
        self.grid.scroll_up(origin..self.scroll_region.end, lines);
    }

    fn deccolm(&mut self) {
        // Setting 132 column font makes no sense, but run the other side effects
        // Clear scrolling region
        let scroll_region = Line(0)..self.grid.num_lines();
        self.set_scrolling_region(scroll_region);

        // Clear grid
        let template = self.empty_cell;
        self.grid.clear(|c| c.reset(&template));
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
    /// Set the window title
    #[inline]
    fn set_title(&mut self, title: &str) {
        self.next_title = Some(title.to_owned());
    }

    /// A character to be displayed
    #[inline]
    fn input(&mut self, c: char) {
        if self.input_needs_wrap {
            if !self.mode.contains(mode::LINE_WRAP) {
                return;
            }

            trace!("wrapping");

            {
                let location = Point {
                    line: self.cursor.point.line,
                    col: self.cursor.point.col
                };

                let cell = &mut self.grid[&location];
                cell.flags.insert(cell::WRAPLINE);
            }

            if (self.cursor.point.line + 1) >= self.scroll_region.end {
                self.linefeed();
            } else {
                self.cursor.point.line += 1;
            }

            self.cursor.point.col = Column(0);
            self.input_needs_wrap = false;
        }

        {
            // Number of cells the char will occupy
            if let Some(width) = c.width() {
                // Sigh, borrowck making us check the width twice. Hopefully the
                // optimizer can fix it.
                let num_cols = self.grid.num_cols();
                {
                    // If in insert mode, first shift cells to the right.
                    if self.mode.contains(mode::INSERT) && self.cursor.point.col + width < num_cols {
                        let line = self.cursor.point.line; // borrowck
                        let col = self.cursor.point.col;
                        let line = &mut self.grid[line];

                        let src = line[col..].as_ptr();
                        let dst = line[(col + width)..].as_mut_ptr();
                        unsafe {
                            // memmove
                            ptr::copy(src, dst, (num_cols - col - width).0);
                        }
                    }

                    let cell = &mut self.grid[&self.cursor.point];
                    *cell = self.cursor.template;
                    cell.c = self.cursor.charsets[self.active_charset].map(c);

                    // Handle wide chars
                    if width == 2 {
                        cell.flags.insert(cell::WIDE_CHAR);
                    }
                }

                // Set spacer cell for wide chars.
                if width == 2 {
                    if self.cursor.point.col + 1 < num_cols {
                        self.cursor.point.col += 1;
                        let spacer = &mut self.grid[&self.cursor.point];
                        *spacer = self.cursor.template;
                        spacer.flags.insert(cell::WIDE_CHAR_SPACER);
                    }
                }
            }
        }

        if (self.cursor.point.col + 1) < self.grid.num_cols() {
            self.cursor.point.col += 1;
        } else {
            self.input_needs_wrap = true;
        }

    }

    #[inline]
    fn dectest(&mut self) {
        trace!("dectest");
        let mut template = self.cursor.template;
        template.c = 'E';

        for row in &mut self.grid.lines_mut() {
            for cell in row {
                cell.reset(&template);
            }
        }
    }

    #[inline]
    fn goto(&mut self, line: Line, col: Column) {
        trace!("goto: line={}, col={}", line, col);
        let (y_offset, max_y) = if self.mode.contains(mode::ORIGIN) {
            (self.scroll_region.start, self.scroll_region.end - 1)
        } else {
            (Line(0), self.grid.num_lines() - 1)
        };

        self.cursor.point.line = min(line + y_offset, max_y);
        self.cursor.point.col = min(col, self.grid.num_cols() - 1);
        self.input_needs_wrap = false;
    }

    #[inline]
    fn goto_line(&mut self, line: Line) {
        trace!("goto_line: {}", line);
        let col = self.cursor.point.col; // borrowck
        self.goto(line, col)
    }

    #[inline]
    fn goto_col(&mut self, col: Column) {
        trace!("goto_col: {}", col);
        let line = self.cursor.point.line; // borrowck
        self.goto(line, col)
    }

    #[inline]
    fn insert_blank(&mut self, count: Column) {
        // Ensure inserting within terminal bounds

        let count = min(count, self.size_info.cols() - self.cursor.point.col);

        let source = self.cursor.point.col;
        let destination = self.cursor.point.col + count;
        let num_cells = (self.size_info.cols() - destination).0;

        let line = self.cursor.point.line; // borrowck
        let line = &mut self.grid[line];

        unsafe {
            let src = line[source..].as_ptr();
            let dst = line[destination..].as_mut_ptr();

            ptr::copy(src, dst, num_cells);
        }

        // Cells were just moved out towards the end of the line; fill in
        // between source and dest with blanks.
        let template = self.cursor.template;
        for c in &mut line[source..destination] {
            c.reset(&template);
        }
    }

    #[inline]
    fn move_up(&mut self, lines: Line) {
        trace!("move_up: {}", lines);
        let move_to = Line(self.cursor.point.line.0.saturating_sub(lines.0));
        let col = self.cursor.point.col; // borrowck
        self.goto(move_to, col)
    }

    #[inline]
    fn move_down(&mut self, lines: Line) {
        trace!("move_down: {}", lines);
        let move_to = self.cursor.point.line + lines;
        let col = self.cursor.point.col; // borrowck
        self.goto(move_to, col)
    }

    #[inline]
    fn move_forward(&mut self, cols: Column) {
        trace!("move_forward: {}", cols);
        self.cursor.point.col = min(self.cursor.point.col + cols, self.grid.num_cols() - 1);
        self.input_needs_wrap = false;
    }

    #[inline]
    fn move_backward(&mut self, cols: Column) {
        trace!("move_backward: {}", cols);
        self.cursor.point.col -= min(self.cursor.point.col, cols);
        self.input_needs_wrap = false;
    }

    #[inline]
    fn identify_terminal<W: io::Write>(&mut self, writer: &mut W) {
        let _ = writer.write_all(b"\x1b[?6c");
    }

    #[inline]
    fn device_status<W: io::Write>(&mut self, writer: &mut W, arg: usize) {
        trace!("device status: {}", arg);
        match arg {
            5 => {
                let _ = writer.write_all(b"\x1b[0n");
            },
            6 => {
                let pos = self.cursor.point;
                let _ = write!(writer, "\x1b[{};{}R", pos.line + 1, pos.col + 1);
            },
            _ => debug!("unknown device status query: {}", arg),
        };
    }

    #[inline]
    fn move_down_and_cr(&mut self, lines: Line) {
        trace!("[unimplemented] move_down_and_cr: {}", lines);
    }

    #[inline]
    fn move_up_and_cr(&mut self, lines: Line) {
        trace!("[unimplemented] move_up_and_cr: {}", lines);
    }

    #[inline]
    fn put_tab(&mut self, mut count: i64) {
        trace!("put_tab: {}", count);

        let mut col = self.cursor.point.col;
        while col < self.grid.num_cols() && count != 0 {
            count -= 1;
            loop {
                if (col + 1) == self.grid.num_cols() {
                    break;
                }

                col += 1;

                if self.tabs[*col as usize] {
                    break;
                }
            }
        }

        self.cursor.point.col = col;
        self.input_needs_wrap = false;
    }

    /// Backspace `count` characters
    #[inline]
    fn backspace(&mut self) {
        trace!("backspace");
        if self.cursor.point.col > Column(0) {
            self.cursor.point.col -= 1;
            self.input_needs_wrap = false;
        }
    }

    /// Carriage return
    #[inline]
    fn carriage_return(&mut self) {
        trace!("carriage_return");
        self.cursor.point.col = Column(0);
        self.input_needs_wrap = false;
    }

    /// Linefeed
    #[inline]
    fn linefeed(&mut self) {
        trace!("linefeed");
        if (self.cursor.point.line + 1) >= self.scroll_region.end {
            self.scroll_up(Line(1));
        } else {
            self.cursor.point.line += 1;
        }
    }

    /// Set current position as a tabstop
    #[inline]
    fn bell(&mut self) {
        trace!("bell");
        self.visual_bell.ring();
    }

    #[inline]
    fn substitute(&mut self) {
        trace!("[unimplemented] substitute");
    }

    /// Run LF/NL
    ///
    /// LF/NL mode has some interesting history. According to ECMA-48 4th
    /// edition, in LINE FEED mode,
    ///
    /// > The execution of the formator functions LINE FEED (LF), FORM FEED
    /// (FF), LINE TABULATION (VT) cause only movement of the active position in
    /// the direction of the line progression.
    ///
    /// In NEW LINE mode,
    ///
    /// > The execution of the formator functions LINE FEED (LF), FORM FEED
    /// (FF), LINE TABULATION (VT) cause movement to the line home position on
    /// the following line, the following form, etc. In the case of LF this is
    /// referred to as the New Line (NL) option.
    ///
    /// Additionally, ECMA-48 4th edition says that this option is deprecated.
    /// ECMA-48 5th edition only mentions this option (without explanation)
    /// saying that it's been removed.
    ///
    /// As an emulator, we need to support it since applications may still rely
    /// on it.
    #[inline]
    fn newline(&mut self) {
        self.linefeed();

        if self.mode.contains(mode::LINE_FEED_NEW_LINE) {
            self.carriage_return();
        }
    }

    #[inline]
    fn set_horizontal_tabstop(&mut self) {
        trace!("set_horizontal_tabstop");
        let column = self.cursor.point.col;
        self.tabs[column.0] = true;
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
        trace!("insert_blank_lines: {}", lines);
        if self.scroll_region.contains_(self.cursor.point.line) {
            let origin = self.cursor.point.line;
            self.scroll_down_relative(origin, lines);
        }
    }

    #[inline]
    fn delete_lines(&mut self, lines: Line) {
        trace!("delete_lines: {}", lines);
        if self.scroll_region.contains_(self.cursor.point.line) {
            let origin = self.cursor.point.line;
            self.scroll_up_relative(origin, lines);
        }
    }

    #[inline]
    fn erase_chars(&mut self, count: Column) {
        trace!("erase_chars: {}, {}", count, self.cursor.point.col);
        let start = self.cursor.point.col;
        let end = min(start + count, self.grid.num_cols() - 1);

        let row = &mut self.grid[self.cursor.point.line];
        let template = self.empty_cell;
        for c in &mut row[start..end] {
            c.reset(&template);
        }
    }

    #[inline]
    fn delete_chars(&mut self, count: Column) {
        // Ensure deleting within terminal bounds
        let count = min(count, self.size_info.cols());

        let start = self.cursor.point.col;
        let end = min(start + count, self.grid.num_cols() - 1);
        let n = (self.size_info.cols() - end).0;

        let line = self.cursor.point.line; // borrowck
        let line = &mut self.grid[line];

        unsafe {
            let src = line[end..].as_ptr();
            let dst = line[start..].as_mut_ptr();

            ptr::copy(src, dst, n);
        }

        // Clear last `count` cells in line. If deleting 1 char, need to delete
        // 1 cell.
        let template = self.empty_cell;
        let end = self.size_info.cols() - count;
        for c in &mut line[end..] {
            c.reset(&template);
        }
    }

    #[inline]
    fn move_backward_tabs(&mut self, count: i64) {
        trace!("[unimplemented] move_backward_tabs: {}", count);
    }

    #[inline]
    fn move_forward_tabs(&mut self, count: i64) {
        trace!("[unimplemented] move_forward_tabs: {}", count);
    }

    #[inline]
    fn save_cursor_position(&mut self) {
        trace!("CursorSave");
        let mut holder = if self.alt {
            &mut self.cursor_save_alt
        } else {
            &mut self.cursor_save
        };

        *holder = self.cursor;
    }

    #[inline]
    fn restore_cursor_position(&mut self) {
        trace!("CursorRestore");
        let holder = if self.alt {
            &self.cursor_save_alt
        } else {
            &self.cursor_save
        };

        self.cursor = *holder;
        self.cursor.point.line = min(self.cursor.point.line, self.grid.num_lines() - 1);
        self.cursor.point.col = min(self.cursor.point.col, self.grid.num_cols() - 1);
    }

    #[inline]
    fn clear_line(&mut self, mode: ansi::LineClearMode) {
        trace!("clear_line: {:?}", mode);
        let mut template = self.cursor.template;
        template.flags ^= template.flags;

        let col =  self.cursor.point.col;

        match mode {
            ansi::LineClearMode::Right => {
                let row = &mut self.grid[self.cursor.point.line];
                for cell in &mut row[col..] {
                    cell.reset(&template);
                }
            },
            ansi::LineClearMode::Left => {
                let row = &mut self.grid[self.cursor.point.line];
                for cell in &mut row[..(col + 1)] {
                    cell.reset(&template);
                }
            },
            ansi::LineClearMode::All => {
                let row = &mut self.grid[self.cursor.point.line];
                for cell in &mut row[..] {
                    cell.reset(&template);
                }
            },
        }
    }

    /// Set the indexed color value
    ///
    /// TODO needs access to `Config`, and `Config` should not overwrite values
    ///      when reloading
    #[inline]
    fn set_color(&mut self, index: usize, color: Rgb) {
        trace!("set_color[{}] = {:?}", index, color);
        self.colors[index] = color;
    }

    #[inline]
    fn clear_screen(&mut self, mode: ansi::ClearMode) {
        trace!("clear_screen: {:?}", mode);
        let mut template = self.cursor.template;
        template.flags ^= template.flags;

        match mode {
            ansi::ClearMode::Below => {
                for cell in &mut self.grid[self.cursor.point.line][self.cursor.point.col..] {
                    cell.reset(&template);
                }
                if self.cursor.point.line < self.grid.num_lines() - 1 {
                    for row in &mut self.grid[(self.cursor.point.line + 1)..] {
                        for cell in row {
                            cell.reset(&template);
                        }
                    }
                }
            },
            ansi::ClearMode::All => {
                self.grid.clear(|c| c.reset(&template));
            },
            ansi::ClearMode::Above => {
                // If clearing more than one line
                if self.cursor.point.line > Line(1) {
                    // Fully clear all lines before the current line
                    for row in &mut self.grid[..self.cursor.point.line] {
                        for cell in row {
                            cell.reset(&template);
                        }
                    }
                }
                // Clear up to the current column in the current line
                let end = min(self.cursor.point.col + 1, self.grid.num_cols());
                for cell in &mut self.grid[self.cursor.point.line][..end] {
                    cell.reset(&template);
                }
            },
            // If scrollback is implemented, this should clear it
            ansi::ClearMode::Saved => return
        }
    }

    #[inline]
    fn clear_tabs(&mut self, mode: ansi::TabulationClearMode) {
        trace!("clear_tabs: {:?}", mode);
        match mode {
            ansi::TabulationClearMode::Current => {
                let column = self.cursor.point.col;
                self.tabs[column.0] = false;
            },
            ansi::TabulationClearMode::All => {
                let len = self.tabs.len();
                // Safe since false boolean is null, each item occupies only 1
                // byte, and called on the length of the vec.
                unsafe {
                    ::std::ptr::write_bytes(self.tabs.as_mut_ptr(), 0, len);
                }
            }
        }
    }

    #[inline]
    fn reset_state(&mut self) {
        trace!("[unimplemented] reset_state");
    }

    #[inline]
    fn reverse_index(&mut self) {
        trace!("reverse_index");
        // if cursor is at the top
        if self.cursor.point.line == self.scroll_region.start {
            self.scroll_down(Line(1));
        } else {
            self.cursor.point.line -= min(self.cursor.point.line, Line(1));
        }
    }

    /// set a terminal attribute
    #[inline]
    fn terminal_attribute(&mut self, attr: Attr) {
        trace!("Set Attribute: {:?}", attr);
        match attr {
            Attr::Foreground(color) => self.cursor.template.fg = color,
            Attr::Background(color) => self.cursor.template.bg = color,
            Attr::Reset => {
                self.cursor.template.fg = Color::Named(NamedColor::Foreground);
                self.cursor.template.bg = Color::Named(NamedColor::Background);
                self.cursor.template.flags = cell::Flags::empty();
            },
            Attr::Reverse => self.cursor.template.flags.insert(cell::INVERSE),
            Attr::CancelReverse => self.cursor.template.flags.remove(cell::INVERSE),
            Attr::Bold => self.cursor.template.flags.insert(cell::BOLD),
            Attr::CancelBoldDim => self.cursor.template.flags.remove(cell::BOLD),
            Attr::Italic => self.cursor.template.flags.insert(cell::ITALIC),
            Attr::CancelItalic => self.cursor.template.flags.remove(cell::ITALIC),
            Attr::Underscore => self.cursor.template.flags.insert(cell::UNDERLINE),
            Attr::CancelUnderline => self.cursor.template.flags.remove(cell::UNDERLINE),
            _ => {
                debug!("Term got unhandled attr: {:?}", attr);
            }
        }
    }

    #[inline]
    fn set_mode(&mut self, mode: ansi::Mode) {
        trace!("set_mode: {:?}", mode);
        match mode {
            ansi::Mode::SwapScreenAndSetRestoreCursor => {
                self.save_cursor_position();
                self.swap_alt();
                self.save_cursor_position();
            },
            ansi::Mode::ShowCursor => self.mode.insert(mode::SHOW_CURSOR),
            ansi::Mode::CursorKeys => self.mode.insert(mode::APP_CURSOR),
            ansi::Mode::ReportMouseClicks => self.mode.insert(mode::MOUSE_REPORT_CLICK),
            ansi::Mode::ReportMouseMotion => self.mode.insert(mode::MOUSE_MOTION),
            ansi::Mode::ReportFocusInOut => self.mode.insert(mode::FOCUS_IN_OUT),
            ansi::Mode::BracketedPaste => self.mode.insert(mode::BRACKETED_PASTE),
            ansi::Mode::SgrMouse => self.mode.insert(mode::SGR_MOUSE),
            ansi::Mode::LineWrap => self.mode.insert(mode::LINE_WRAP),
            ansi::Mode::LineFeedNewLine => self.mode.insert(mode::LINE_FEED_NEW_LINE),
            ansi::Mode::Origin => self.mode.insert(mode::ORIGIN),
            ansi::Mode::DECCOLM => self.deccolm(),
            ansi::Mode::Insert => self.mode.insert(mode::INSERT), // heh
            _ => {
                trace!(".. ignoring set_mode");
            }
        }
    }

    #[inline]
    fn unset_mode(&mut self,mode: ansi::Mode) {
        trace!("unset_mode: {:?}", mode);
        match mode {
            ansi::Mode::SwapScreenAndSetRestoreCursor => {
                self.restore_cursor_position();
                self.swap_alt();
                self.restore_cursor_position();
            },
            ansi::Mode::ShowCursor => self.mode.remove(mode::SHOW_CURSOR),
            ansi::Mode::CursorKeys => self.mode.remove(mode::APP_CURSOR),
            ansi::Mode::ReportMouseClicks => self.mode.remove(mode::MOUSE_REPORT_CLICK),
            ansi::Mode::ReportMouseMotion => self.mode.remove(mode::MOUSE_MOTION),
            ansi::Mode::ReportFocusInOut => self.mode.remove(mode::FOCUS_IN_OUT),
            ansi::Mode::BracketedPaste => self.mode.remove(mode::BRACKETED_PASTE),
            ansi::Mode::SgrMouse => self.mode.remove(mode::SGR_MOUSE),
            ansi::Mode::LineWrap => self.mode.remove(mode::LINE_WRAP),
            ansi::Mode::LineFeedNewLine => self.mode.remove(mode::LINE_FEED_NEW_LINE),
            ansi::Mode::Origin => self.mode.remove(mode::ORIGIN),
            ansi::Mode::DECCOLM => self.deccolm(),
            ansi::Mode::Insert => self.mode.remove(mode::INSERT),
            _ => {
                trace!(".. ignoring unset_mode");
            }
        }
    }

    #[inline]
    fn set_scrolling_region(&mut self, region: Range<Line>) {
        trace!("set scroll region: {:?}", region);
        self.scroll_region.start = min(region.start, self.grid.num_lines());
        self.scroll_region.end = min(region.end, self.grid.num_lines());
        self.goto(Line(0), Column(0));
    }

    #[inline]
    fn set_keypad_application_mode(&mut self) {
        trace!("set mode::APP_KEYPAD");
        self.mode.insert(mode::APP_KEYPAD);
    }

    #[inline]
    fn unset_keypad_application_mode(&mut self) {
        trace!("unset mode::APP_KEYPAD");
        self.mode.remove(mode::APP_KEYPAD);
    }

    #[inline]
    fn configure_charset(&mut self, index: CharsetIndex, charset: StandardCharset) {
        trace!("designate {:?} character set as {:?}", index, charset);
        self.cursor.charsets[index] = charset;
    }

    #[inline]
    fn set_active_charset(&mut self, index: CharsetIndex) {
        trace!("Activate {:?} character set", index);
        self.active_charset = index;
    }
}

#[cfg(test)]
mod tests {
    extern crate serde_json;

    use super::{Cell, Term, SizeInfo};
    use term::cell;

    use grid::Grid;
    use index::{Point, Line, Column};
    use ansi::{Handler, CharsetIndex, StandardCharset};
    use selection::Selection;
    use std::mem;

    #[test]
    fn semantic_selection_works() {
        let size = SizeInfo {
            width: 21.0,
            height: 51.0,
            cell_width: 3.0,
            cell_height: 3.0,
            padding_x: 0.0,
            padding_y: 0.0,
        };
        let mut term = Term::new(&Default::default(), size);
        let mut grid: Grid<Cell> = Grid::new(Line(3), Column(5), &Cell::default());
        for i in 0..5 {
            for j in 0..2 {
                grid[Line(j)][Column(i)].c = 'a';
            }
        }
        grid[Line(0)][Column(0)].c = '"';
        grid[Line(0)][Column(3)].c = '"';
        grid[Line(1)][Column(2)].c = '"';
        grid[Line(0)][Column(4)].flags.insert(cell::WRAPLINE);

        let mut escape_chars = String::from("\"");

        mem::swap(&mut term.grid, &mut grid);
        mem::swap(&mut term.semantic_escape_chars, &mut escape_chars);

        {
            let mut selection = Selection::new();
            term.semantic_selection(&mut selection, Point { line: Line(0), col: Column(1) });
            assert_eq!(term.string_from_selection(&selection.span().unwrap()), "aa");
        }

        {
            let mut selection = Selection::new();
            term.semantic_selection(&mut selection, Point { line: Line(0), col: Column(4) });
            assert_eq!(term.string_from_selection(&selection.span().unwrap()), "aaa");
        }

        {
            let mut selection = Selection::new();
            term.semantic_selection(&mut selection, Point { line: Line(1), col: Column(1) });
            assert_eq!(term.string_from_selection(&selection.span().unwrap()), "aaa");
        }
    }

    #[test]
    fn line_selection_works() {
        let size = SizeInfo {
            width: 21.0,
            height: 51.0,
            cell_width: 3.0,
            cell_height: 3.0,
            padding_x: 0.0,
            padding_y: 0.0,
        };
        let mut term = Term::new(&Default::default(), size);
        let mut grid: Grid<Cell> = Grid::new(Line(1), Column(5), &Cell::default());
        for i in 0..5 {
            grid[Line(0)][Column(i)].c = 'a';
        }
        grid[Line(0)][Column(0)].c = '"';
        grid[Line(0)][Column(3)].c = '"';


        mem::swap(&mut term.grid, &mut grid);

        let mut selection = Selection::new();
        term.line_selection(&mut selection, Point { line: Line(0), col: Column(3) });
        match selection.span() {
            Some(span) => assert_eq!(term.string_from_selection(&span), "\"aa\"a"),
            _ => ()
        }
    }

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
    fn input_line_drawing_character() {
        let size = SizeInfo {
            width: 21.0,
            height: 51.0,
            cell_width: 3.0,
            cell_height: 3.0,
            padding_x: 0.0,
            padding_y: 0.0,
        };
        let mut term = Term::new(&Default::default(), size);
        let cursor = Point::new(Line(0), Column(0));
        term.configure_charset(CharsetIndex::G0,
                               StandardCharset::SpecialCharacterAndLineDrawing);
        term.input('a');

        assert_eq!(term.grid()[&cursor].c, '▒');
    }
}

#[cfg(all(test, feature = "bench"))]
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

        let mut terminal = Term::new(&Default::default(), size);
        mem::swap(&mut terminal.grid, &mut grid);

        b.iter(|| {
            let iter = terminal.renderable_cells(&Selection::Empty);
            for cell in iter {
                test::black_box(cell);
            }
        })
    }
}
