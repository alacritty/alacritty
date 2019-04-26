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
use std::cmp::{max, min};
use std::ops::{Index, IndexMut, Range, RangeInclusive};
use std::time::{Duration, Instant};
use std::{io, mem, ptr};

use copypasta::{Clipboard, Load, Store};
use font::{self, RasterizedGlyph, Size};
use glutin::MouseCursor;
use unicode_width::UnicodeWidthChar;

use crate::ansi::{
    self, Attr, CharsetIndex, Color, CursorStyle, Handler, NamedColor, StandardCharset,
};
use crate::config::{Config, VisualBellAnimation};
use crate::cursor;
use crate::grid::{
    BidirectionalIterator, DisplayIter, Grid, GridCell, IndexRegion, Indexed, Scroll,
    ViewportPosition,
};
use crate::index::{self, Column, Contains, IndexRange, Line, Linear, Point};
use crate::input::FONT_SIZE_STEP;
use crate::message_bar::MessageBuffer;
use crate::selection::{self, Locations, Selection};
use crate::term::cell::{Cell, Flags, LineLength};
use crate::term::color::Rgb;
use crate::url::{Url, UrlParser};

#[cfg(windows)]
use crate::tty;

pub mod cell;
pub mod color;

/// A type that can expand a given point to a region
///
/// Usually this is implemented for some 2-D array type since
/// points are two dimensional indices.
pub trait Search {
    /// Find the nearest semantic boundary _to the left_ of provided point.
    fn semantic_search_left(&self, _: Point<usize>) -> Point<usize>;
    /// Find the nearest semantic boundary _to the point_ of provided point.
    fn semantic_search_right(&self, _: Point<usize>) -> Point<usize>;
    /// Find the nearest URL boundary in both directions.
    fn url_search(&self, _: Point<usize>) -> Option<Url>;
}

impl Search for Term {
    fn semantic_search_left(&self, mut point: Point<usize>) -> Point<usize> {
        // Limit the starting point to the last line in the history
        point.line = min(point.line, self.grid.len() - 1);

        let mut iter = self.grid.iter_from(point);
        let last_col = self.grid.num_cols() - Column(1);

        while let Some(cell) = iter.prev() {
            if self.semantic_escape_chars.contains(cell.c) {
                break;
            }

            if iter.cur.col == last_col && !cell.flags.contains(cell::Flags::WRAPLINE) {
                break; // cut off if on new line or hit escape char
            }

            point = iter.cur;
        }

        point
    }

    fn semantic_search_right(&self, mut point: Point<usize>) -> Point<usize> {
        // Limit the starting point to the last line in the history
        point.line = min(point.line, self.grid.len() - 1);

        let mut iter = self.grid.iter_from(point);
        let last_col = self.grid.num_cols() - 1;

        while let Some(cell) = iter.next() {
            if self.semantic_escape_chars.contains(cell.c) {
                break;
            }

            point = iter.cur;

            if point.col == last_col && !cell.flags.contains(cell::Flags::WRAPLINE) {
                break; // cut off if on new line or hit escape char
            }
        }

        point
    }

    fn url_search(&self, mut point: Point<usize>) -> Option<Url> {
        let last_col = self.grid.num_cols() - 1;

        // Switch first line from top to bottom
        point.line = self.grid.num_lines().0 - point.line - 1;

        // Remove viewport scroll offset
        point.line += self.grid.display_offset();

        // Create forwards and backwards iterators
        let mut iterf = self.grid.iter_from(point);
        point.col += 1;
        let mut iterb = self.grid.iter_from(point);

        // Find URLs
        let mut url_parser = UrlParser::new();
        while let Some(cell) = iterb.prev() {
            if (iterb.cur.col == last_col && !cell.flags.contains(cell::Flags::WRAPLINE))
                || url_parser.advance_left(cell)
            {
                break;
            }
        }

        while let Some(cell) = iterf.next() {
            if url_parser.advance_right(cell)
                || (iterf.cur.col == last_col && !cell.flags.contains(cell::Flags::WRAPLINE))
            {
                break;
            }
        }
        url_parser.url()
    }
}

impl selection::Dimensions for Term {
    fn dimensions(&self) -> Point {
        Point { col: self.grid.num_cols(), line: self.grid.num_lines() }
    }
}

/// Iterator that yields cells needing render
///
/// Yields cells that require work to be displayed (that is, not a an empty
/// background cell). Additionally, this manages some state of the grid only
/// relevant for rendering like temporarily changing the cell with the cursor.
///
/// This manages the cursor during a render. The cursor location is inverted to
/// draw it, and reverted after drawing to maintain state.
pub struct RenderableCellsIter<'a> {
    inner: DisplayIter<'a, Cell>,
    grid: &'a Grid<Cell>,
    cursor: &'a Point,
    cursor_offset: usize,
    cursor_cell: Option<RasterizedGlyph>,
    cursor_style: CursorStyle,
    config: &'a Config,
    colors: &'a color::List,
    selection: Option<RangeInclusive<index::Linear>>,
    url_highlight: &'a Option<RangeInclusive<index::Linear>>,
}

impl<'a> RenderableCellsIter<'a> {
    /// Create the renderable cells iterator
    ///
    /// The cursor and terminal mode are required for properly displaying the
    /// cursor.
    fn new<'b>(
        term: &'b Term,
        config: &'b Config,
        selection: Option<Locations>,
        mut cursor_style: CursorStyle,
        metrics: font::Metrics,
    ) -> RenderableCellsIter<'b> {
        let grid = &term.grid;

        let cursor_offset = grid.line_to_offset(term.cursor.point.line);
        let inner = grid.display_iter();

        let mut selection_range = None;
        if let Some(loc) = selection {
            // Get on-screen lines of the selection's locations
            let start_line = grid.buffer_line_to_visible(loc.start.line);
            let end_line = grid.buffer_line_to_visible(loc.end.line);

            // Get start/end locations based on what part of selection is on screen
            let locations = match (start_line, end_line) {
                (ViewportPosition::Visible(start_line), ViewportPosition::Visible(end_line)) => {
                    Some((start_line, loc.start.col, end_line, loc.end.col))
                },
                (ViewportPosition::Visible(start_line), ViewportPosition::Above) => {
                    Some((start_line, loc.start.col, Line(0), Column(0)))
                },
                (ViewportPosition::Below, ViewportPosition::Visible(end_line)) => {
                    Some((grid.num_lines(), Column(0), end_line, loc.end.col))
                },
                (ViewportPosition::Below, ViewportPosition::Above) => {
                    Some((grid.num_lines(), Column(0), Line(0), Column(0)))
                },
                _ => None,
            };

            if let Some((start_line, start_col, end_line, end_col)) = locations {
                // start and end *lines* are swapped as we switch from buffer to
                // Line coordinates.
                let mut end = Point { line: start_line, col: start_col };
                let mut start = Point { line: end_line, col: end_col };

                if start > end {
                    ::std::mem::swap(&mut start, &mut end);
                }

                let cols = grid.num_cols();
                let start = Linear::from_point(cols, start.into());
                let end = Linear::from_point(cols, end.into());

                // Update the selection
                selection_range = Some(RangeInclusive::new(start, end));
            }
        }

        // Load cursor glyph
        let cursor = &term.cursor.point;
        let cursor_visible = term.mode.contains(TermMode::SHOW_CURSOR) && grid.contains(cursor);
        let cursor_cell = if cursor_visible {
            let offset_x = config.font().offset().x;
            let offset_y = config.font().offset().y;

            let is_wide = grid[cursor].flags.contains(cell::Flags::WIDE_CHAR)
                && (cursor.col + 1) < grid.num_cols();
            Some(cursor::get_cursor_glyph(cursor_style, metrics, offset_x, offset_y, is_wide))
        } else {
            // Use hidden cursor so text will not get inverted
            cursor_style = CursorStyle::Hidden;
            None
        };

        RenderableCellsIter {
            cursor,
            cursor_offset,
            grid,
            inner,
            selection: selection_range,
            url_highlight: &grid.url_highlight,
            config,
            colors: &term.colors,
            cursor_cell,
            cursor_style,
        }
    }
}

#[derive(Clone, Debug)]
pub enum RenderableCellContent {
    Chars([char; cell::MAX_ZEROWIDTH_CHARS + 1]),
    Cursor((CursorStyle, RasterizedGlyph)),
}

#[derive(Clone, Debug)]
pub struct RenderableCell {
    /// A _Display_ line (not necessarily an _Active_ line)
    pub line: Line,
    pub column: Column,
    pub inner: RenderableCellContent,
    pub fg: Rgb,
    pub bg: Rgb,
    pub bg_alpha: f32,
    pub flags: cell::Flags,
}

impl RenderableCell {
    fn new(config: &Config, colors: &color::List, cell: Indexed<Cell>, selected: bool) -> Self {
        // Lookup RGB values
        let mut fg_rgb = Self::compute_fg_rgb(config, colors, cell.fg, cell.flags);
        let mut bg_rgb = Self::compute_bg_rgb(colors, cell.bg);

        let selection_background = config.colors().selection.background;
        if let (true, Some(col)) = (selected, selection_background) {
            // Override selection background with config colors
            bg_rgb = col;
        } else if selected ^ cell.inverse() {
            if fg_rgb == bg_rgb && !cell.flags.contains(Flags::HIDDEN) {
                // Reveal inversed text when fg/bg is the same
                fg_rgb = colors[NamedColor::Background];
                bg_rgb = colors[NamedColor::Foreground];
            } else {
                // Invert cell fg and bg colors
                mem::swap(&mut fg_rgb, &mut bg_rgb);
            }
        }

        // Override selection text with config colors
        if let (true, Some(col)) = (selected, config.colors().selection.text) {
            fg_rgb = col;
        }

        RenderableCell {
            line: cell.line,
            column: cell.column,
            inner: RenderableCellContent::Chars(cell.chars()),
            fg: fg_rgb,
            bg: bg_rgb,
            bg_alpha: Self::compute_bg_alpha(colors, bg_rgb),
            flags: cell.flags,
        }
    }

    fn compute_fg_rgb(config: &Config, colors: &color::List, fg: Color, flags: cell::Flags) -> Rgb {
        match fg {
            Color::Spec(rgb) => rgb,
            Color::Named(ansi) => {
                match (config.draw_bold_text_with_bright_colors(), flags & Flags::DIM_BOLD) {
                    // If no bright foreground is set, treat it like the BOLD flag doesn't exist
                    (_, cell::Flags::DIM_BOLD)
                        if ansi == NamedColor::Foreground
                            && config.colors().primary.bright_foreground.is_none() =>
                    {
                        colors[NamedColor::DimForeground]
                    },
                    // Draw bold text in bright colors *and* contains bold flag.
                    (true, cell::Flags::BOLD) => colors[ansi.to_bright()],
                    // Cell is marked as dim and not bold
                    (_, cell::Flags::DIM) | (false, cell::Flags::DIM_BOLD) => colors[ansi.to_dim()],
                    // None of the above, keep original color.
                    _ => colors[ansi],
                }
            },
            Color::Indexed(idx) => {
                let idx = match (
                    config.draw_bold_text_with_bright_colors(),
                    flags & Flags::DIM_BOLD,
                    idx,
                ) {
                    (true, cell::Flags::BOLD, 0..=7) => idx as usize + 8,
                    (false, cell::Flags::DIM, 8..=15) => idx as usize - 8,
                    (false, cell::Flags::DIM, 0..=7) => idx as usize + 260,
                    _ => idx as usize,
                };

                colors[idx]
            },
        }
    }

    #[inline]
    fn compute_bg_alpha(colors: &color::List, bg: Rgb) -> f32 {
        if colors[NamedColor::Background] == bg {
            0.
        } else {
            1.
        }
    }

    #[inline]
    fn compute_bg_rgb(colors: &color::List, bg: Color) -> Rgb {
        match bg {
            Color::Spec(rgb) => rgb,
            Color::Named(ansi) => colors[ansi],
            Color::Indexed(idx) => colors[idx],
        }
    }
}

impl<'a> Iterator for RenderableCellsIter<'a> {
    type Item = RenderableCell;

    /// Gets the next renderable cell
    ///
    /// Skips empty (background) cells and applies any flags to the cell state
    /// (eg. invert fg and bg colors).
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.cursor_offset == self.inner.offset() && self.inner.column() == self.cursor.col {
                // Handle cursor
                if let Some(cursor_cell) = self.cursor_cell.take() {
                    let cell = Indexed {
                        inner: self.grid[self.cursor],
                        column: self.cursor.col,
                        line: self.cursor.line,
                    };
                    let mut renderable_cell =
                        RenderableCell::new(self.config, self.colors, cell, false);

                    renderable_cell.inner =
                        RenderableCellContent::Cursor((self.cursor_style, cursor_cell));

                    if let Some(color) = self.config.cursor_cursor_color() {
                        renderable_cell.fg = color;
                    }

                    return Some(renderable_cell);
                } else {
                    let mut cell =
                        RenderableCell::new(self.config, self.colors, self.inner.next()?, false);

                    if self.cursor_style == CursorStyle::Block {
                        std::mem::swap(&mut cell.bg, &mut cell.fg);

                        if let Some(color) = self.config.cursor_text_color() {
                            cell.fg = color;
                        }
                    }

                    return Some(cell);
                }
            } else {
                let mut cell = self.inner.next()?;

                let index = Linear::new(self.grid.num_cols(), cell.column, cell.line);

                let selected =
                    self.selection.as_ref().map(|range| range.contains_(index)).unwrap_or(false);

                // Underline URL highlights
                if self.url_highlight.as_ref().map(|range| range.contains_(index)).unwrap_or(false)
                {
                    cell.inner.flags.insert(Flags::UNDERLINE);
                } else if cell.is_empty() && !selected {
                    continue;
                }

                return Some(RenderableCell::new(self.config, self.colors, cell, selected));
            }
        }
    }
}

pub mod mode {
    use bitflags::bitflags;

    bitflags! {
        pub struct TermMode: u16 {
            const SHOW_CURSOR         = 0b00_0000_0000_0001;
            const APP_CURSOR          = 0b00_0000_0000_0010;
            const APP_KEYPAD          = 0b00_0000_0000_0100;
            const MOUSE_REPORT_CLICK  = 0b00_0000_0000_1000;
            const BRACKETED_PASTE     = 0b00_0000_0001_0000;
            const SGR_MOUSE           = 0b00_0000_0010_0000;
            const MOUSE_MOTION        = 0b00_0000_0100_0000;
            const LINE_WRAP           = 0b00_0000_1000_0000;
            const LINE_FEED_NEW_LINE  = 0b00_0001_0000_0000;
            const ORIGIN              = 0b00_0010_0000_0000;
            const INSERT              = 0b00_0100_0000_0000;
            const FOCUS_IN_OUT        = 0b00_1000_0000_0000;
            const ALT_SCREEN          = 0b01_0000_0000_0000;
            const MOUSE_DRAG          = 0b10_0000_0000_0000;
            const ANY                 = 0b11_1111_1111_1111;
            const NONE                = 0;
        }
    }

    impl Default for TermMode {
        fn default() -> TermMode {
            TermMode::SHOW_CURSOR | TermMode::LINE_WRAP
        }
    }
}

pub use crate::term::mode::TermMode;

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
            StandardCharset::SpecialCharacterAndLineDrawing => match c {
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
                _ => c,
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
    pub point: Point,

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
    (1.0 - x).powi(3) * p0
        + 3.0 * (1.0 - x).powi(2) * x * p1
        + 3.0 * (1.0 - x) * x.powi(2) * p2
        + x.powi(3) * p3
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

    /// Get the currently intensity of the visual bell. The bell's intensity
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
            None => true,
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
                let elapsed_f =
                    elapsed.as_secs() as f64 + f64::from(elapsed.subsec_nanos()) / 1e9f64;
                let duration_f = self.duration.as_secs() as f64
                    + f64::from(self.duration.subsec_nanos()) / 1e9f64;

                // Otherwise, we compute a value `time` from 0.0 to 1.0
                // inclusive that represents the ratio of `elapsed` time to the
                // `duration` of the VisualBell.
                let time = (elapsed_f / duration_f).min(1.0);

                // We use this to compute the inverse `intensity` of the
                // VisualBell. When `time` is 0.0, `inverse_intensity` is 0.0,
                // and when `time` is 1.0, `inverse_intensity` is 1.0.
                let inverse_intensity = match self.animation {
                    VisualBellAnimation::Ease | VisualBellAnimation::EaseOut => {
                        cubic_bezier(0.25, 0.1, 0.25, 1.0, time)
                    },
                    VisualBellAnimation::EaseOutSine => cubic_bezier(0.39, 0.575, 0.565, 1.0, time),
                    VisualBellAnimation::EaseOutQuad => cubic_bezier(0.25, 0.46, 0.45, 0.94, time),
                    VisualBellAnimation::EaseOutCubic => {
                        cubic_bezier(0.215, 0.61, 0.355, 1.0, time)
                    },
                    VisualBellAnimation::EaseOutQuart => cubic_bezier(0.165, 0.84, 0.44, 1.0, time),
                    VisualBellAnimation::EaseOutQuint => cubic_bezier(0.23, 1.0, 0.32, 1.0, time),
                    VisualBellAnimation::EaseOutExpo => cubic_bezier(0.19, 1.0, 0.22, 1.0, time),
                    VisualBellAnimation::EaseOutCirc => cubic_bezier(0.075, 0.82, 0.165, 1.0, time),
                    VisualBellAnimation::Linear => time,
                };

                // Since we want the `intensity` of the VisualBell to decay over
                // `time`, we subtract the `inverse_intensity` from 1.0.
                1.0 - inverse_intensity
            },
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
    /// arrays. Without it we would have to sanitize cursor.col every time we used it.
    input_needs_wrap: bool,

    /// Got a request to set title; it's buffered here until next draw.
    ///
    /// Would be nice to avoid the allocation...
    next_title: Option<String>,

    /// Got a request to set the mouse cursor; it's buffered here until the next draw
    next_mouse_cursor: Option<MouseCursor>,

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
    tabs: TabStops,

    /// Mode flags
    mode: TermMode,

    /// Scroll region
    scroll_region: Range<Line>,

    /// Font size
    pub font_size: Size,
    original_font_size: Size,

    /// Size
    size_info: SizeInfo,

    pub dirty: bool,

    pub visual_bell: VisualBell,
    pub next_is_urgent: Option<bool>,

    /// Saved cursor from main grid
    cursor_save: Cursor,

    /// Saved cursor from alt grid
    cursor_save_alt: Cursor,

    semantic_escape_chars: String,

    /// Colors used for rendering
    colors: color::List,

    /// Is color in `colors` modified or not
    color_modified: [bool; color::COUNT],

    /// Original colors from config
    original_colors: color::List,

    /// Current style of the cursor
    cursor_style: Option<CursorStyle>,

    /// Default style for resetting the cursor
    default_cursor_style: CursorStyle,

    /// Whether to permit updating the terminal title
    dynamic_title: bool,

    /// Number of spaces in one tab
    tabspaces: usize,

    /// Automatically scroll to bottom when new lines are added
    auto_scroll: bool,

    /// Buffer to store messages for the message bar
    message_buffer: MessageBuffer,

    /// Hint that Alacritty should be closed
    should_exit: bool,
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

    /// DPI factor of the current window
    #[serde(default)]
    pub dpr: f64,
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

    pub fn contains_point(&self, x: usize, y: usize, include_padding: bool) -> bool {
        if include_padding {
            x < self.width as usize && y < self.height as usize
        } else {
            x < (self.width - self.padding_x) as usize
                && x >= self.padding_x as usize
                && y < (self.height - self.padding_y) as usize
                && y >= self.padding_y as usize
        }
    }

    pub fn pixels_to_coords(&self, x: usize, y: usize) -> Point {
        let col = Column(x.saturating_sub(self.padding_x as usize) / (self.cell_width as usize));
        let line = Line(y.saturating_sub(self.padding_y as usize) / (self.cell_height as usize));

        Point {
            line: min(line, Line(self.lines().saturating_sub(1))),
            col: min(col, Column(self.cols().saturating_sub(1))),
        }
    }
}

impl Term {
    pub fn selection(&self) -> &Option<Selection> {
        &self.grid.selection
    }

    pub fn selection_mut(&mut self) -> &mut Option<Selection> {
        &mut self.grid.selection
    }

    #[inline]
    pub fn get_next_title(&mut self) -> Option<String> {
        self.next_title.take()
    }

    #[inline]
    pub fn scroll_display(&mut self, scroll: Scroll) {
        self.grid.scroll_display(scroll);
        self.reset_url_highlight();
        self.dirty = true;
    }

    #[inline]
    pub fn get_next_mouse_cursor(&mut self) -> Option<MouseCursor> {
        self.next_mouse_cursor.take()
    }

    pub fn new(config: &Config, size: SizeInfo, message_buffer: MessageBuffer) -> Term {
        let num_cols = size.cols();
        let num_lines = size.lines();

        let history_size = config.scrolling().history as usize;
        let grid = Grid::new(num_lines, num_cols, history_size, Cell::default());
        let alt = Grid::new(num_lines, num_cols, 0 /* scroll history */, Cell::default());

        let tabspaces = config.tabspaces();
        let tabs = TabStops::new(grid.num_cols(), tabspaces);

        let scroll_region = Line(0)..grid.num_lines();

        let colors = color::List::from(config.colors());

        Term {
            next_title: None,
            next_mouse_cursor: None,
            dirty: false,
            visual_bell: VisualBell::new(config),
            next_is_urgent: None,
            input_needs_wrap: false,
            grid,
            alt_grid: alt,
            alt: false,
            font_size: config.font().size(),
            original_font_size: config.font().size(),
            active_charset: Default::default(),
            cursor: Default::default(),
            cursor_save: Default::default(),
            cursor_save_alt: Default::default(),
            tabs,
            mode: Default::default(),
            scroll_region,
            size_info: size,
            colors,
            color_modified: [false; color::COUNT],
            original_colors: colors,
            semantic_escape_chars: config.selection().semantic_escape_chars.clone(),
            cursor_style: None,
            default_cursor_style: config.cursor_style(),
            dynamic_title: config.dynamic_title(),
            tabspaces,
            auto_scroll: config.scrolling().auto_scroll,
            message_buffer,
            should_exit: false,
        }
    }

    pub fn change_font_size(&mut self, delta: f32) {
        // Saturating addition with minimum font size FONT_SIZE_STEP
        let new_size = self.font_size + Size::new(delta);
        self.font_size = max(new_size, Size::new(FONT_SIZE_STEP));
        self.dirty = true;
    }

    pub fn reset_font_size(&mut self) {
        self.font_size = self.original_font_size;
        self.dirty = true;
    }

    pub fn update_config(&mut self, config: &Config) {
        self.semantic_escape_chars = config.selection().semantic_escape_chars.clone();
        self.original_colors.fill_named(config.colors());
        self.original_colors.fill_cube(config.colors());
        self.original_colors.fill_gray_ramp(config.colors());
        for i in 0..color::COUNT {
            if !self.color_modified[i] {
                self.colors[i] = self.original_colors[i];
            }
        }
        self.visual_bell.update_config(config);
        self.default_cursor_style = config.cursor_style();
        self.dynamic_title = config.dynamic_title();
        self.auto_scroll = config.scrolling().auto_scroll;
        self.grid.update_history(config.scrolling().history as usize, &self.cursor.template);
    }

    #[inline]
    pub fn needs_draw(&self) -> bool {
        self.dirty
    }

    pub fn selection_to_string(&self) -> Option<String> {
        /// Need a generic push() for the Append trait
        trait PushChar {
            fn push_char(&mut self, c: char);
            fn maybe_newline(&mut self, grid: &Grid<Cell>, line: usize, ending: Column) {
                if ending != Column(0)
                    && !grid[line][ending - 1].flags.contains(cell::Flags::WRAPLINE)
                {
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

        trait Append: PushChar {
            fn append(
                &mut self,
                grid: &Grid<Cell>,
                tabs: &TabStops,
                line: usize,
                cols: Range<Column>,
            );
        }

        impl Append for String {
            fn append(
                &mut self,
                grid: &Grid<Cell>,
                tabs: &TabStops,
                mut line: usize,
                cols: Range<Column>,
            ) {
                // Select until last line still within the buffer
                line = min(line, grid.len() - 1);

                let grid_line = &grid[line];
                let line_length = grid_line.line_length();
                let line_end = min(line_length, cols.end + 1);

                if line_end.0 == 0 && cols.end >= grid.num_cols() - 1 {
                    self.push('\n');
                } else if cols.start < line_end {
                    let mut tab_mode = false;

                    for col in IndexRange::from(cols.start..line_end) {
                        let cell = grid_line[col];

                        if tab_mode {
                            // Skip over whitespace until next tab-stop once a tab was found
                            if tabs[col] {
                                tab_mode = false;
                            } else if cell.c == ' ' {
                                continue;
                            }
                        }

                        if !cell.flags.contains(cell::Flags::WIDE_CHAR_SPACER) {
                            self.push(cell.c);
                            for c in (&cell.chars()[1..]).iter().filter(|c| **c != ' ') {
                                self.push(*c);
                            }
                        }

                        if cell.c == '\t' {
                            tab_mode = true;
                        }
                    }

                    if cols.end >= grid.num_cols() - 1 {
                        self.maybe_newline(grid, line, line_end);
                    }
                }
            }
        }

        let alt_screen = self.mode.contains(TermMode::ALT_SCREEN);
        let selection = self.grid.selection.clone()?;
        let span = selection.to_span(self, alt_screen)?;

        let mut res = String::new();

        let Locations { mut start, mut end } = span.to_locations();

        if start > end {
            ::std::mem::swap(&mut start, &mut end);
        }

        let line_count = end.line - start.line;
        let max_col = Column(usize::max_value() - 1);

        match line_count {
            // Selection within single line
            0 => {
                res.append(&self.grid, &self.tabs, start.line, start.col..end.col);
            },

            // Selection ends on line following start
            1 => {
                // Ending line
                res.append(&self.grid, &self.tabs, end.line, end.col..max_col);

                // Starting line
                res.append(&self.grid, &self.tabs, start.line, Column(0)..start.col);
            },

            // Multi line selection
            _ => {
                // Ending line
                res.append(&self.grid, &self.tabs, end.line, end.col..max_col);

                let middle_range = (start.line + 1)..(end.line);
                for line in middle_range.rev() {
                    res.append(&self.grid, &self.tabs, line, Column(0)..max_col);
                }

                // Starting line
                res.append(&self.grid, &self.tabs, start.line, Column(0)..start.col);
            },
        }

        Some(res)
    }

    pub(crate) fn visible_to_buffer(&self, point: Point) -> Point<usize> {
        self.grid.visible_to_buffer(point)
    }

    /// Convert the given pixel values to a grid coordinate
    ///
    /// The mouse coordinates are expected to be relative to the top left. The
    /// line and column returned are also relative to the top left.
    ///
    /// Returns None if the coordinates are outside the window,
    /// padding pixels are considered inside the window
    pub fn pixels_to_coords(&self, x: usize, y: usize) -> Option<Point> {
        if self.size_info.contains_point(x, y, true) {
            Some(self.size_info.pixels_to_coords(x, y))
        } else {
            None
        }
    }

    /// Access to the raw grid data structure
    ///
    /// This is a bit of a hack; when the window is closed, the event processor
    /// serializes the grid state to a file.
    pub fn grid(&self) -> &Grid<Cell> {
        &self.grid
    }

    /// Mutable access for swapping out the grid during tests
    #[cfg(test)]
    pub fn grid_mut(&mut self) -> &mut Grid<Cell> {
        &mut self.grid
    }

    /// Iterate over the *renderable* cells in the terminal
    ///
    /// A renderable cell is any cell which has content other than the default
    /// background color.  Cells with an alternate background color are
    /// considered renderable as are cells with any text content.
    pub fn renderable_cells<'b>(
        &'b self,
        config: &'b Config,
        window_focused: bool,
        metrics: font::Metrics,
    ) -> RenderableCellsIter<'_> {
        let alt_screen = self.mode.contains(TermMode::ALT_SCREEN);
        let selection = self
            .grid
            .selection
            .as_ref()
            .and_then(|s| s.to_span(self, alt_screen))
            .map(|span| span.to_locations());

        let cursor = if window_focused || !config.unfocused_hollow_cursor() {
            self.cursor_style.unwrap_or(self.default_cursor_style)
        } else {
            CursorStyle::HollowBlock
        };

        RenderableCellsIter::new(&self, config, selection, cursor, metrics)
    }

    /// Resize terminal to new dimensions
    pub fn resize(&mut self, size: &SizeInfo) {
        debug!("Resizing terminal");

        // Bounds check; lots of math assumes width and height are > 0
        if size.width as usize <= 2 * self.size_info.padding_x as usize
            || size.height as usize <= 2 * self.size_info.padding_y as usize
        {
            return;
        }

        let old_cols = self.grid.num_cols();
        let old_lines = self.grid.num_lines();
        let mut num_cols = size.cols();
        let mut num_lines = size.lines();

        if let Some(message) = self.message_buffer.message() {
            num_lines -= message.text(size).len();
        }

        self.size_info = *size;

        if old_cols == num_cols && old_lines == num_lines {
            debug!("Term::resize dimensions unchanged");
            return;
        }

        self.grid.selection = None;
        self.alt_grid.selection = None;
        self.grid.url_highlight = None;

        // Should not allow less than 1 col, causes all sorts of checks to be required.
        if num_cols <= Column(1) {
            num_cols = Column(2);
        }

        // Should not allow less than 1 line, causes all sorts of checks to be required.
        if num_lines <= Line(1) {
            num_lines = Line(2);
        }

        // Scroll up to keep cursor in terminal
        if self.cursor.point.line >= num_lines {
            let lines = self.cursor.point.line - num_lines + 1;
            self.grid.scroll_up(&(Line(0)..old_lines), lines, &self.cursor.template);
        }

        // Scroll up alt grid as well
        if self.cursor_save_alt.point.line >= num_lines {
            let lines = self.cursor_save_alt.point.line - num_lines + 1;
            self.alt_grid.scroll_up(&(Line(0)..old_lines), lines, &self.cursor_save_alt.template);
        }

        // Move prompt down when growing if scrollback lines are available
        if num_lines > old_lines {
            if self.mode.contains(TermMode::ALT_SCREEN) {
                let growage = min(num_lines - old_lines, Line(self.alt_grid.scroll_limit()));
                self.cursor_save.point.line += growage;
            } else {
                let growage = min(num_lines - old_lines, Line(self.grid.scroll_limit()));
                self.cursor.point.line += growage;
            }
        }

        debug!("New num_cols is {} and num_lines is {}", num_cols, num_lines);

        // Resize grids to new size
        let alt_cursor_point = if self.mode.contains(TermMode::ALT_SCREEN) {
            &mut self.cursor_save.point
        } else {
            &mut self.cursor_save_alt.point
        };
        self.grid.resize(num_lines, num_cols, &mut self.cursor.point, &Cell::default());
        self.alt_grid.resize(num_lines, num_cols, alt_cursor_point, &Cell::default());

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
        self.tabs = TabStops::new(self.grid.num_cols(), self.tabspaces);
    }

    #[inline]
    pub fn size_info(&self) -> &SizeInfo {
        &self.size_info
    }

    #[inline]
    pub fn mode(&self) -> &TermMode {
        &self.mode
    }

    #[inline]
    pub fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    pub fn swap_alt(&mut self) {
        if self.alt {
            let template = &self.cursor.template;
            self.grid.region_mut(..).each(|c| c.reset(template));
        }

        self.alt = !self.alt;
        ::std::mem::swap(&mut self.grid, &mut self.alt_grid);
    }

    /// Scroll screen down
    ///
    /// Text moves down; clear at bottom
    /// Expects origin to be in scroll range.
    #[inline]
    fn scroll_down_relative(&mut self, origin: Line, mut lines: Line) {
        trace!("Scrolling down relative: origin={}, lines={}", origin, lines);
        lines = min(lines, self.scroll_region.end - self.scroll_region.start);
        lines = min(lines, self.scroll_region.end - origin);

        // Scroll between origin and bottom
        let mut template = self.cursor.template;
        template.flags = Flags::empty();
        self.grid.scroll_down(&(origin..self.scroll_region.end), lines, &template);
    }

    /// Scroll screen up
    ///
    /// Text moves up; clear at top
    /// Expects origin to be in scroll range.
    #[inline]
    fn scroll_up_relative(&mut self, origin: Line, lines: Line) {
        trace!("Scrolling up relative: origin={}, lines={}", origin, lines);
        let lines = min(lines, self.scroll_region.end - self.scroll_region.start);

        // Scroll from origin to bottom less number of lines
        let mut template = self.cursor.template;
        template.flags = Flags::empty();
        self.grid.scroll_up(&(origin..self.scroll_region.end), lines, &template);
    }

    fn deccolm(&mut self) {
        // Setting 132 column font makes no sense, but run the other side effects
        // Clear scrolling region
        let scroll_region = Line(0)..self.grid.num_lines();
        self.set_scrolling_region(scroll_region);

        // Clear grid
        let template = self.cursor.template;
        self.grid.region_mut(..).each(|c| c.reset(&template));
    }

    #[inline]
    pub fn background_color(&self) -> Rgb {
        self.colors[NamedColor::Background]
    }

    #[inline]
    pub fn message_buffer_mut(&mut self) -> &mut MessageBuffer {
        &mut self.message_buffer
    }

    #[inline]
    pub fn message_buffer(&self) -> &MessageBuffer {
        &self.message_buffer
    }

    #[inline]
    pub fn exit(&mut self) {
        self.should_exit = true;
    }

    #[inline]
    pub fn should_exit(&self) -> bool {
        self.should_exit
    }

    #[inline]
    pub fn set_url_highlight(&mut self, hl: RangeInclusive<index::Linear>) {
        self.grid.url_highlight = Some(hl);
    }

    #[inline]
    pub fn reset_url_highlight(&mut self) {
        let mouse_mode =
            TermMode::MOUSE_MOTION | TermMode::MOUSE_DRAG | TermMode::MOUSE_REPORT_CLICK;
        let mouse_cursor = if self.mode().intersects(mouse_mode) {
            MouseCursor::Default
        } else {
            MouseCursor::Text
        };
        self.set_mouse_cursor(mouse_cursor);

        self.grid.url_highlight = None;
        self.dirty = true;
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
        if self.dynamic_title {
            self.next_title = Some(title.to_owned());

            #[cfg(windows)]
            {
                // cmd.exe in winpty: winpty incorrectly sets the title to ' ' instead of
                // 'Alacritty' - thus we have to substitute this back to get equivalent
                // behaviour as conpty.
                //
                // The starts_with check is necessary because other shells e.g. bash set a
                // different title and don't need Alacritty prepended.
                if !tty::is_conpty() && title.starts_with(' ') {
                    self.next_title = Some(format!("Alacritty {}", title.trim()));
                }
            }
        }
    }

    /// Set the mouse cursor
    #[inline]
    fn set_mouse_cursor(&mut self, cursor: MouseCursor) {
        self.next_mouse_cursor = Some(cursor);
    }

    /// A character to be displayed
    #[inline]
    fn input(&mut self, c: char) {
        // If enabled, scroll to bottom when character is received
        if self.auto_scroll {
            self.scroll_display(Scroll::Bottom);
        }

        if self.input_needs_wrap {
            if !self.mode.contains(TermMode::LINE_WRAP) {
                return;
            }

            trace!("Wrapping input");

            {
                let location = Point { line: self.cursor.point.line, col: self.cursor.point.col };

                let cell = &mut self.grid[&location];
                cell.flags.insert(cell::Flags::WRAPLINE);
            }

            if (self.cursor.point.line + 1) >= self.scroll_region.end {
                self.linefeed();
            } else {
                self.cursor.point.line += 1;
            }

            self.cursor.point.col = Column(0);
            self.input_needs_wrap = false;
        }

        // Number of cells the char will occupy
        if let Some(width) = c.width() {
            let num_cols = self.grid.num_cols();

            // If in insert mode, first shift cells to the right.
            if self.mode.contains(TermMode::INSERT) && self.cursor.point.col + width < num_cols {
                let line = self.cursor.point.line;
                let col = self.cursor.point.col;
                let line = &mut self.grid[line];

                let src = line[col..].as_ptr();
                let dst = line[(col + width)..].as_mut_ptr();
                unsafe {
                    // memmove
                    ptr::copy(src, dst, (num_cols - col - width).0);
                }
            }

            // Handle zero-width characters
            if width == 0 {
                let mut col = self.cursor.point.col.0.saturating_sub(1);
                let line = self.cursor.point.line;
                if self.grid[line][Column(col)].flags.contains(cell::Flags::WIDE_CHAR_SPACER) {
                    col = col.saturating_sub(1);
                }
                self.grid[line][Column(col)].push_extra(c);
                return;
            }

            let cell = &mut self.grid[&self.cursor.point];
            *cell = self.cursor.template;
            cell.c = self.cursor.charsets[self.active_charset].map(c);

            // Handle wide chars
            if width == 2 {
                cell.flags.insert(cell::Flags::WIDE_CHAR);

                if self.cursor.point.col + 1 < num_cols {
                    self.cursor.point.col += 1;
                    let spacer = &mut self.grid[&self.cursor.point];
                    *spacer = self.cursor.template;
                    spacer.flags.insert(cell::Flags::WIDE_CHAR_SPACER);
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
        trace!("Dectesting");
        let mut template = self.cursor.template;
        template.c = 'E';

        self.grid.region_mut(..).each(|c| c.reset(&template));
    }

    #[inline]
    fn goto(&mut self, line: Line, col: Column) {
        trace!("Going to: line={}, col={}", line, col);
        let (y_offset, max_y) = if self.mode.contains(TermMode::ORIGIN) {
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
        trace!("Going to line: {}", line);
        self.goto(line, self.cursor.point.col)
    }

    #[inline]
    fn goto_col(&mut self, col: Column) {
        trace!("Going to column: {}", col);
        self.goto(self.cursor.point.line, col)
    }

    #[inline]
    fn insert_blank(&mut self, count: Column) {
        // Ensure inserting within terminal bounds

        let count = min(count, self.size_info.cols() - self.cursor.point.col);

        let source = self.cursor.point.col;
        let destination = self.cursor.point.col + count;
        let num_cells = (self.size_info.cols() - destination).0;

        let line = &mut self.grid[self.cursor.point.line];

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
        trace!("Moving up: {}", lines);
        let move_to = Line(self.cursor.point.line.0.saturating_sub(lines.0));
        self.goto(move_to, self.cursor.point.col)
    }

    #[inline]
    fn move_down(&mut self, lines: Line) {
        trace!("Moving down: {}", lines);
        let move_to = self.cursor.point.line + lines;
        self.goto(move_to, self.cursor.point.col)
    }

    #[inline]
    fn move_forward(&mut self, cols: Column) {
        trace!("Moving forward: {}", cols);
        self.cursor.point.col = min(self.cursor.point.col + cols, self.grid.num_cols() - 1);
        self.input_needs_wrap = false;
    }

    #[inline]
    fn move_backward(&mut self, cols: Column) {
        trace!("Moving backward: {}", cols);
        self.cursor.point.col -= min(self.cursor.point.col, cols);
        self.input_needs_wrap = false;
    }

    #[inline]
    fn identify_terminal<W: io::Write>(&mut self, writer: &mut W) {
        let _ = writer.write_all(b"\x1b[?6c");
    }

    #[inline]
    fn device_status<W: io::Write>(&mut self, writer: &mut W, arg: usize) {
        trace!("Reporting device status: {}", arg);
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
        trace!("Moving down and cr: {}", lines);
        let move_to = self.cursor.point.line + lines;
        self.goto(move_to, Column(0))
    }

    #[inline]
    fn move_up_and_cr(&mut self, lines: Line) {
        trace!("Moving up and cr: {}", lines);
        let move_to = Line(self.cursor.point.line.0.saturating_sub(lines.0));
        self.goto(move_to, Column(0))
    }

    #[inline]
    fn put_tab(&mut self, mut count: i64) {
        trace!("Putting tab: {}", count);

        while self.cursor.point.col < self.grid.num_cols() && count != 0 {
            count -= 1;

            let cell = &mut self.grid[&self.cursor.point];
            if cell.c == ' ' {
                cell.c = self.cursor.charsets[self.active_charset].map('\t');
            }

            loop {
                if (self.cursor.point.col + 1) == self.grid.num_cols() {
                    break;
                }

                self.cursor.point.col += 1;

                if self.tabs[self.cursor.point.col] {
                    break;
                }
            }
        }

        self.input_needs_wrap = false;
    }

    /// Backspace `count` characters
    #[inline]
    fn backspace(&mut self) {
        trace!("Backspace");
        if self.cursor.point.col > Column(0) {
            self.cursor.point.col -= 1;
            self.input_needs_wrap = false;
        }
    }

    /// Carriage return
    #[inline]
    fn carriage_return(&mut self) {
        trace!("Carriage return");
        self.cursor.point.col = Column(0);
        self.input_needs_wrap = false;
    }

    /// Linefeed
    #[inline]
    fn linefeed(&mut self) {
        trace!("Linefeed");
        let next = self.cursor.point.line + 1;
        if next == self.scroll_region.end {
            self.scroll_up(Line(1));
        } else if next < self.grid.num_lines() {
            self.cursor.point.line += 1;
        }
    }

    /// Set current position as a tabstop
    #[inline]
    fn bell(&mut self) {
        trace!("Bell");
        self.visual_bell.ring();
        self.next_is_urgent = Some(true);
    }

    #[inline]
    fn substitute(&mut self) {
        trace!("[unimplemented] Substitute");
    }

    /// Run LF/NL
    ///
    /// LF/NL mode has some interesting history. According to ECMA-48 4th
    /// edition, in LINE FEED mode,
    ///
    /// > The execution of the formatter functions LINE FEED (LF), FORM FEED
    /// (FF), LINE TABULATION (VT) cause only movement of the active position in
    /// the direction of the line progression.
    ///
    /// In NEW LINE mode,
    ///
    /// > The execution of the formatter functions LINE FEED (LF), FORM FEED
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

        if self.mode.contains(TermMode::LINE_FEED_NEW_LINE) {
            self.carriage_return();
        }
    }

    #[inline]
    fn set_horizontal_tabstop(&mut self) {
        trace!("Setting horizontal tabstop");
        let column = self.cursor.point.col;
        self.tabs[column] = true;
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
        trace!("Inserting blank {} lines", lines);
        if self.scroll_region.contains_(self.cursor.point.line) {
            let origin = self.cursor.point.line;
            self.scroll_down_relative(origin, lines);
        }
    }

    #[inline]
    fn delete_lines(&mut self, lines: Line) {
        trace!("Deleting {} lines", lines);
        if self.scroll_region.contains_(self.cursor.point.line) {
            let origin = self.cursor.point.line;
            self.scroll_up_relative(origin, lines);
        }
    }

    #[inline]
    fn erase_chars(&mut self, count: Column) {
        trace!("Erasing chars: count={}, col={}", count, self.cursor.point.col);
        let start = self.cursor.point.col;
        let end = min(start + count, self.grid.num_cols());

        let row = &mut self.grid[self.cursor.point.line];
        let template = self.cursor.template; // Cleared cells have current background color set
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

        let line = &mut self.grid[self.cursor.point.line];

        unsafe {
            let src = line[end..].as_ptr();
            let dst = line[start..].as_mut_ptr();

            ptr::copy(src, dst, n);
        }

        // Clear last `count` cells in line. If deleting 1 char, need to delete
        // 1 cell.
        let template = self.cursor.template;
        let end = self.size_info.cols() - count;
        for c in &mut line[end..] {
            c.reset(&template);
        }
    }

    #[inline]
    fn move_backward_tabs(&mut self, count: i64) {
        trace!("Moving backward {} tabs", count);

        for _ in 0..count {
            let mut col = self.cursor.point.col;
            for i in (0..(col.0)).rev() {
                if self.tabs[index::Column(i)] {
                    col = index::Column(i);
                    break;
                }
            }
            self.cursor.point.col = col;
        }
    }

    #[inline]
    fn move_forward_tabs(&mut self, count: i64) {
        trace!("[unimplemented] Moving forward {} tabs", count);
    }

    #[inline]
    fn save_cursor_position(&mut self) {
        trace!("Saving cursor position");
        let cursor = if self.alt { &mut self.cursor_save_alt } else { &mut self.cursor_save };

        *cursor = self.cursor;
    }

    #[inline]
    fn restore_cursor_position(&mut self) {
        trace!("Restoring cursor position");
        let source = if self.alt { &self.cursor_save_alt } else { &self.cursor_save };

        self.cursor = *source;
        self.cursor.point.line = min(self.cursor.point.line, self.grid.num_lines() - 1);
        self.cursor.point.col = min(self.cursor.point.col, self.grid.num_cols() - 1);
    }

    #[inline]
    fn clear_line(&mut self, mode: ansi::LineClearMode) {
        trace!("Clearing line: {:?}", mode);
        let mut template = self.cursor.template;
        template.flags ^= template.flags;

        let col = self.cursor.point.col;

        match mode {
            ansi::LineClearMode::Right => {
                let row = &mut self.grid[self.cursor.point.line];
                for cell in &mut row[col..] {
                    cell.reset(&template);
                }
            },
            ansi::LineClearMode::Left => {
                let row = &mut self.grid[self.cursor.point.line];
                for cell in &mut row[..=col] {
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
    #[inline]
    fn set_color(&mut self, index: usize, color: Rgb) {
        trace!("Setting color[{}] = {:?}", index, color);
        self.colors[index] = color;
        self.color_modified[index] = true;
    }

    /// Reset the indexed color to original value
    #[inline]
    fn reset_color(&mut self, index: usize) {
        trace!("Reseting color[{}]", index);
        self.colors[index] = self.original_colors[index];
        self.color_modified[index] = false;
    }

    /// Set the clipboard
    #[inline]
    fn set_clipboard(&mut self, string: &str) {
        Clipboard::new().and_then(|mut clipboard| clipboard.store_primary(string)).unwrap_or_else(
            |err| {
                warn!("Error storing selection to clipboard: {}", err);
            },
        );
    }

    #[inline]
    fn clear_screen(&mut self, mode: ansi::ClearMode) {
        trace!("Clearing screen: {:?}", mode);
        let mut template = self.cursor.template;
        template.flags ^= template.flags;

        // Remove active selections and URL highlights
        self.grid.selection = None;
        self.grid.url_highlight = None;

        match mode {
            ansi::ClearMode::Below => {
                for cell in &mut self.grid[self.cursor.point.line][self.cursor.point.col..] {
                    cell.reset(&template);
                }
                if self.cursor.point.line < self.grid.num_lines() - 1 {
                    self.grid
                        .region_mut((self.cursor.point.line + 1)..)
                        .each(|cell| cell.reset(&template));
                }
            },
            ansi::ClearMode::All => self.grid.region_mut(..).each(|c| c.reset(&template)),
            ansi::ClearMode::Above => {
                // If clearing more than one line
                if self.cursor.point.line > Line(1) {
                    // Fully clear all lines before the current line
                    self.grid
                        .region_mut(..self.cursor.point.line)
                        .each(|cell| cell.reset(&template));
                }
                // Clear up to the current column in the current line
                let end = min(self.cursor.point.col + 1, self.grid.num_cols());
                for cell in &mut self.grid[self.cursor.point.line][..end] {
                    cell.reset(&template);
                }
            },
            ansi::ClearMode::Saved => self.grid.clear_history(),
        }
    }

    #[inline]
    fn clear_tabs(&mut self, mode: ansi::TabulationClearMode) {
        trace!("Clearing tabs: {:?}", mode);
        match mode {
            ansi::TabulationClearMode::Current => {
                let column = self.cursor.point.col;
                self.tabs[column] = false;
            },
            ansi::TabulationClearMode::All => {
                self.tabs.clear_all();
            },
        }
    }

    // Reset all important fields in the term struct
    #[inline]
    fn reset_state(&mut self) {
        if self.alt {
            self.swap_alt();
        }
        self.input_needs_wrap = false;
        self.next_title = None;
        self.next_mouse_cursor = None;
        self.cursor = Default::default();
        self.active_charset = Default::default();
        self.mode = Default::default();
        self.font_size = self.original_font_size;
        self.next_is_urgent = None;
        self.cursor_save = Default::default();
        self.cursor_save_alt = Default::default();
        self.colors = self.original_colors;
        self.color_modified = [false; color::COUNT];
        self.cursor_style = None;
        self.grid.reset(&Cell::default());
        self.alt_grid.reset(&Cell::default());
        self.scroll_region = Line(0)..self.grid.num_lines();
    }

    #[inline]
    fn reverse_index(&mut self) {
        trace!("Reversing index");
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
        trace!("Setting attribute: {:?}", attr);
        match attr {
            Attr::Foreground(color) => self.cursor.template.fg = color,
            Attr::Background(color) => self.cursor.template.bg = color,
            Attr::Reset => {
                self.cursor.template.fg = Color::Named(NamedColor::Foreground);
                self.cursor.template.bg = Color::Named(NamedColor::Background);
                self.cursor.template.flags = cell::Flags::empty();
            },
            Attr::Reverse => self.cursor.template.flags.insert(cell::Flags::INVERSE),
            Attr::CancelReverse => self.cursor.template.flags.remove(cell::Flags::INVERSE),
            Attr::Bold => self.cursor.template.flags.insert(cell::Flags::BOLD),
            Attr::CancelBold => self.cursor.template.flags.remove(cell::Flags::BOLD),
            Attr::Dim => self.cursor.template.flags.insert(cell::Flags::DIM),
            Attr::CancelBoldDim => {
                self.cursor.template.flags.remove(cell::Flags::BOLD | cell::Flags::DIM)
            },
            Attr::Italic => self.cursor.template.flags.insert(cell::Flags::ITALIC),
            Attr::CancelItalic => self.cursor.template.flags.remove(cell::Flags::ITALIC),
            Attr::Underscore => self.cursor.template.flags.insert(cell::Flags::UNDERLINE),
            Attr::CancelUnderline => self.cursor.template.flags.remove(cell::Flags::UNDERLINE),
            Attr::Hidden => self.cursor.template.flags.insert(cell::Flags::HIDDEN),
            Attr::CancelHidden => self.cursor.template.flags.remove(cell::Flags::HIDDEN),
            Attr::Strike => self.cursor.template.flags.insert(cell::Flags::STRIKEOUT),
            Attr::CancelStrike => self.cursor.template.flags.remove(cell::Flags::STRIKEOUT),
            _ => {
                debug!("Term got unhandled attr: {:?}", attr);
            },
        }
    }

    #[inline]
    fn set_mode(&mut self, mode: ansi::Mode) {
        trace!("Setting mode: {:?}", mode);
        match mode {
            ansi::Mode::SwapScreenAndSetRestoreCursor => {
                if !self.alt {
                    self.mode.insert(TermMode::ALT_SCREEN);
                    self.save_cursor_position();
                    self.swap_alt();
                    self.save_cursor_position();
                }
            },
            ansi::Mode::ShowCursor => self.mode.insert(TermMode::SHOW_CURSOR),
            ansi::Mode::CursorKeys => self.mode.insert(TermMode::APP_CURSOR),
            ansi::Mode::ReportMouseClicks => {
                self.mode.insert(TermMode::MOUSE_REPORT_CLICK);
                self.set_mouse_cursor(MouseCursor::Default);
            },
            ansi::Mode::ReportCellMouseMotion => {
                self.mode.insert(TermMode::MOUSE_DRAG);
                self.set_mouse_cursor(MouseCursor::Default);
            },
            ansi::Mode::ReportAllMouseMotion => {
                self.mode.insert(TermMode::MOUSE_MOTION);
                self.set_mouse_cursor(MouseCursor::Default);
            },
            ansi::Mode::ReportFocusInOut => self.mode.insert(TermMode::FOCUS_IN_OUT),
            ansi::Mode::BracketedPaste => self.mode.insert(TermMode::BRACKETED_PASTE),
            ansi::Mode::SgrMouse => self.mode.insert(TermMode::SGR_MOUSE),
            ansi::Mode::LineWrap => self.mode.insert(TermMode::LINE_WRAP),
            ansi::Mode::LineFeedNewLine => self.mode.insert(TermMode::LINE_FEED_NEW_LINE),
            ansi::Mode::Origin => self.mode.insert(TermMode::ORIGIN),
            ansi::Mode::DECCOLM => self.deccolm(),
            ansi::Mode::Insert => self.mode.insert(TermMode::INSERT), // heh
            ansi::Mode::BlinkingCursor => {
                trace!("... unimplemented mode");
            },
        }
    }

    #[inline]
    fn unset_mode(&mut self, mode: ansi::Mode) {
        trace!("Unsetting mode: {:?}", mode);
        match mode {
            ansi::Mode::SwapScreenAndSetRestoreCursor => {
                if self.alt {
                    self.mode.remove(TermMode::ALT_SCREEN);
                    self.restore_cursor_position();
                    self.swap_alt();
                    self.restore_cursor_position();
                }
            },
            ansi::Mode::ShowCursor => self.mode.remove(TermMode::SHOW_CURSOR),
            ansi::Mode::CursorKeys => self.mode.remove(TermMode::APP_CURSOR),
            ansi::Mode::ReportMouseClicks => {
                self.mode.remove(TermMode::MOUSE_REPORT_CLICK);
                self.set_mouse_cursor(MouseCursor::Text);
            },
            ansi::Mode::ReportCellMouseMotion => {
                self.mode.remove(TermMode::MOUSE_DRAG);
                self.set_mouse_cursor(MouseCursor::Text);
            },
            ansi::Mode::ReportAllMouseMotion => {
                self.mode.remove(TermMode::MOUSE_MOTION);
                self.set_mouse_cursor(MouseCursor::Text);
            },
            ansi::Mode::ReportFocusInOut => self.mode.remove(TermMode::FOCUS_IN_OUT),
            ansi::Mode::BracketedPaste => self.mode.remove(TermMode::BRACKETED_PASTE),
            ansi::Mode::SgrMouse => self.mode.remove(TermMode::SGR_MOUSE),
            ansi::Mode::LineWrap => self.mode.remove(TermMode::LINE_WRAP),
            ansi::Mode::LineFeedNewLine => self.mode.remove(TermMode::LINE_FEED_NEW_LINE),
            ansi::Mode::Origin => self.mode.remove(TermMode::ORIGIN),
            ansi::Mode::DECCOLM => self.deccolm(),
            ansi::Mode::Insert => self.mode.remove(TermMode::INSERT),
            ansi::Mode::BlinkingCursor => {
                trace!("... unimplemented mode");
            },
        }
    }

    #[inline]
    fn set_scrolling_region(&mut self, region: Range<Line>) {
        trace!("Setting scrolling region: {:?}", region);
        self.scroll_region.start = min(region.start, self.grid.num_lines());
        self.scroll_region.end = min(region.end, self.grid.num_lines());
        self.goto(Line(0), Column(0));
    }

    #[inline]
    fn set_keypad_application_mode(&mut self) {
        trace!("Setting keypad application mode");
        self.mode.insert(TermMode::APP_KEYPAD);
    }

    #[inline]
    fn unset_keypad_application_mode(&mut self) {
        trace!("Unsetting keypad application mode");
        self.mode.remove(TermMode::APP_KEYPAD);
    }

    #[inline]
    fn configure_charset(&mut self, index: CharsetIndex, charset: StandardCharset) {
        trace!("Configuring charset {:?} as {:?}", index, charset);
        self.cursor.charsets[index] = charset;
    }

    #[inline]
    fn set_active_charset(&mut self, index: CharsetIndex) {
        trace!("Setting active charset {:?}", index);
        self.active_charset = index;
    }

    #[inline]
    fn set_cursor_style(&mut self, style: Option<CursorStyle>) {
        trace!("Setting cursor style {:?}", style);
        self.cursor_style = style;
    }
}

struct TabStops {
    tabs: Vec<bool>,
}

impl TabStops {
    fn new(num_cols: Column, tabspaces: usize) -> TabStops {
        TabStops {
            tabs: IndexRange::from(Column(0)..num_cols)
                .map(|i| (*i as usize) % tabspaces == 0)
                .collect::<Vec<bool>>(),
        }
    }

    fn clear_all(&mut self) {
        unsafe {
            ptr::write_bytes(self.tabs.as_mut_ptr(), 0, self.tabs.len());
        }
    }
}

impl Index<Column> for TabStops {
    type Output = bool;

    fn index(&self, index: Column) -> &bool {
        &self.tabs[index.0]
    }
}

impl IndexMut<Column> for TabStops {
    fn index_mut(&mut self, index: Column) -> &mut bool {
        self.tabs.index_mut(index.0)
    }
}

#[cfg(test)]
mod tests {
    use serde_json;

    use super::{Cell, SizeInfo, Term};
    use crate::term::cell;

    use crate::ansi::{self, CharsetIndex, Handler, StandardCharset};
    use crate::config::Config;
    use crate::grid::{Grid, Scroll};
    use crate::index::{Column, Line, Point, Side};
    use crate::input::FONT_SIZE_STEP;
    use crate::message_bar::MessageBuffer;
    use crate::selection::Selection;
    use font::Size;
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
            dpr: 1.0,
        };
        let mut term = Term::new(&Default::default(), size, MessageBuffer::new());
        let mut grid: Grid<Cell> = Grid::new(Line(3), Column(5), 0, Cell::default());
        for i in 0..5 {
            for j in 0..2 {
                grid[Line(j)][Column(i)].c = 'a';
            }
        }
        grid[Line(0)][Column(0)].c = '"';
        grid[Line(0)][Column(3)].c = '"';
        grid[Line(1)][Column(2)].c = '"';
        grid[Line(0)][Column(4)].flags.insert(cell::Flags::WRAPLINE);

        let mut escape_chars = String::from("\"");

        mem::swap(&mut term.grid, &mut grid);
        mem::swap(&mut term.semantic_escape_chars, &mut escape_chars);

        {
            *term.selection_mut() = Some(Selection::semantic(Point { line: 2, col: Column(1) }));
            assert_eq!(term.selection_to_string(), Some(String::from("aa")));
        }

        {
            *term.selection_mut() = Some(Selection::semantic(Point { line: 2, col: Column(4) }));
            assert_eq!(term.selection_to_string(), Some(String::from("aaa")));
        }

        {
            *term.selection_mut() = Some(Selection::semantic(Point { line: 1, col: Column(1) }));
            assert_eq!(term.selection_to_string(), Some(String::from("aaa")));
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
            dpr: 1.0,
        };
        let mut term = Term::new(&Default::default(), size, MessageBuffer::new());
        let mut grid: Grid<Cell> = Grid::new(Line(1), Column(5), 0, Cell::default());
        for i in 0..5 {
            grid[Line(0)][Column(i)].c = 'a';
        }
        grid[Line(0)][Column(0)].c = '"';
        grid[Line(0)][Column(3)].c = '"';

        mem::swap(&mut term.grid, &mut grid);

        *term.selection_mut() = Some(Selection::lines(Point { line: 0, col: Column(3) }));
        assert_eq!(term.selection_to_string(), Some(String::from("\"aa\"a\n")));
    }

    #[test]
    fn selecting_empty_line() {
        let size = SizeInfo {
            width: 21.0,
            height: 51.0,
            cell_width: 3.0,
            cell_height: 3.0,
            padding_x: 0.0,
            padding_y: 0.0,
            dpr: 1.0,
        };
        let mut term = Term::new(&Default::default(), size, MessageBuffer::new());
        let mut grid: Grid<Cell> = Grid::new(Line(3), Column(3), 0, Cell::default());
        for l in 0..3 {
            if l != 1 {
                for c in 0..3 {
                    grid[Line(l)][Column(c)].c = 'a';
                }
            }
        }

        mem::swap(&mut term.grid, &mut grid);

        let mut selection = Selection::simple(Point { line: 2, col: Column(0) }, Side::Left);
        selection.update(Point { line: 0, col: Column(2) }, Side::Right);
        *term.selection_mut() = Some(selection);
        assert_eq!(term.selection_to_string(), Some("aaa\n\naaa\n".into()));
    }

    /// Check that the grid can be serialized back and forth losslessly
    ///
    /// This test is in the term module as opposed to the grid since we want to
    /// test this property with a T=Cell.
    #[test]
    fn grid_serde() {
        let template = Cell::default();

        let grid: Grid<Cell> = Grid::new(Line(24), Column(80), 0, template);
        let serialized = serde_json::to_string(&grid).expect("ser");
        let deserialized = serde_json::from_str::<Grid<Cell>>(&serialized).expect("de");

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
            dpr: 1.0,
        };
        let mut term = Term::new(&Default::default(), size, MessageBuffer::new());
        let cursor = Point::new(Line(0), Column(0));
        term.configure_charset(CharsetIndex::G0, StandardCharset::SpecialCharacterAndLineDrawing);
        term.input('a');

        assert_eq!(term.grid()[&cursor].c, '▒');
    }

    fn change_font_size_works(font_size: f32) {
        let size = SizeInfo {
            width: 21.0,
            height: 51.0,
            cell_width: 3.0,
            cell_height: 3.0,
            padding_x: 0.0,
            padding_y: 0.0,
            dpr: 1.0,
        };
        let config: Config = Default::default();
        let mut term: Term = Term::new(&config, size, MessageBuffer::new());
        term.change_font_size(font_size);

        let expected_font_size: Size = config.font().size() + Size::new(font_size);
        assert_eq!(term.font_size, expected_font_size);
    }

    #[test]
    fn increase_font_size_works() {
        change_font_size_works(10.0);
    }

    #[test]
    fn decrease_font_size_works() {
        change_font_size_works(-10.0);
    }

    #[test]
    fn prevent_font_below_threshold_works() {
        let size = SizeInfo {
            width: 21.0,
            height: 51.0,
            cell_width: 3.0,
            cell_height: 3.0,
            padding_x: 0.0,
            padding_y: 0.0,
            dpr: 1.0,
        };
        let config: Config = Default::default();
        let mut term: Term = Term::new(&config, size, MessageBuffer::new());

        term.change_font_size(-100.0);

        let expected_font_size: Size = Size::new(FONT_SIZE_STEP);
        assert_eq!(term.font_size, expected_font_size);
    }

    #[test]
    fn reset_font_size_works() {
        let size = SizeInfo {
            width: 21.0,
            height: 51.0,
            cell_width: 3.0,
            cell_height: 3.0,
            padding_x: 0.0,
            padding_y: 0.0,
            dpr: 1.0,
        };
        let config: Config = Default::default();
        let mut term: Term = Term::new(&config, size, MessageBuffer::new());

        term.change_font_size(10.0);
        term.reset_font_size();

        let expected_font_size: Size = config.font().size();
        assert_eq!(term.font_size, expected_font_size);
    }

    #[test]
    fn clear_saved_lines() {
        let size = SizeInfo {
            width: 21.0,
            height: 51.0,
            cell_width: 3.0,
            cell_height: 3.0,
            padding_x: 0.0,
            padding_y: 0.0,
            dpr: 1.0,
        };
        let config: Config = Default::default();
        let mut term: Term = Term::new(&config, size, MessageBuffer::new());

        // Add one line of scrollback
        term.grid.scroll_up(&(Line(0)..Line(1)), Line(1), &Cell::default());

        // Clear the history
        term.clear_screen(ansi::ClearMode::Saved);

        // Make sure that scrolling does not change the grid
        let mut scrolled_grid = term.grid.clone();
        scrolled_grid.scroll_display(Scroll::Top);
        assert_eq!(term.grid, scrolled_grid);
    }
}

#[cfg(all(test, feature = "bench"))]
mod benches {
    extern crate serde_json as json;
    extern crate test;

    use std::fs::File;
    use std::io::Read;
    use std::mem;
    use std::path::Path;

    use crate::config::Config;
    use crate::grid::Grid;
    use crate::message_bar::MessageBuffer;

    use super::cell::Cell;
    use super::{SizeInfo, Term};

    fn read_string<P>(path: P) -> String
    where
        P: AsRef<Path>,
    {
        let mut res = String::new();
        File::open(path.as_ref()).unwrap().read_to_string(&mut res).unwrap();

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
        let serialized_grid = read_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ref/vim_large_window_scroll/grid.json"
        ));
        let serialized_size = read_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ref/vim_large_window_scroll/size.json"
        ));

        let mut grid: Grid<Cell> = json::from_str(&serialized_grid).unwrap();
        let size: SizeInfo = json::from_str(&serialized_size).unwrap();

        let config = Config::default();

        let mut terminal = Term::new(&config, size, MessageBuffer::new());
        mem::swap(&mut terminal.grid, &mut grid);

        let metrics = font::Metrics {
            descent: 0.,
            line_height: 0.,
            average_advance: 0.,
            underline_position: 0.,
            underline_thickness: 0.,
            strikeout_position: 0.,
            strikeout_thickness: 0.,
        };

        b.iter(|| {
            let iter = terminal.renderable_cells(&config, false, metrics);
            for cell in iter {
                test::black_box(cell);
            }
        })
    }
}
