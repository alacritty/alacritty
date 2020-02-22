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
use std::ops::{Index, IndexMut, Range};
use std::time::{Duration, Instant};
use std::{io, mem, ptr, str};

use log::{debug, trace};
use serde::{Deserialize, Serialize};
use unicode_width::UnicodeWidthChar;

use crate::ansi::{
    self, Attr, CharsetIndex, Color, CursorStyle, Handler, NamedColor, StandardCharset, TermInfo,
};
use crate::clipboard::{Clipboard, ClipboardType};
use crate::config::{Config, VisualBellAnimation, DEFAULT_NAME};
use crate::event::{Event, EventListener};
use crate::grid::{
    BidirectionalIterator, DisplayIter, Grid, GridCell, IndexRegion, Indexed, Scroll,
};
use crate::index::{self, Column, IndexRange, Line, Point};
use crate::selection::{Selection, SelectionRange};
use crate::term::cell::{Cell, Flags, LineLength};
use crate::term::color::Rgb;
#[cfg(windows)]
use crate::tty;

pub mod cell;
pub mod color;

/// Used to match equal brackets, when performing a bracket-pair selection.
const BRACKET_PAIRS: [(char, char); 4] = [('(', ')'), ('[', ']'), ('{', '}'), ('<', '>')];

/// Max size of the window title stack
const TITLE_STACK_MAX_DEPTH: usize = 4096;

/// A type that can expand a given point to a region
///
/// Usually this is implemented for some 2-D array type since
/// points are two dimensional indices.
pub trait Search {
    /// Find the nearest semantic boundary _to the left_ of provided point.
    fn semantic_search_left(&self, _: Point<usize>) -> Point<usize>;
    /// Find the nearest semantic boundary _to the point_ of provided point.
    fn semantic_search_right(&self, _: Point<usize>) -> Point<usize>;
    /// Find the beginning of a line, following line wraps.
    fn line_search_left(&self, _: Point<usize>) -> Point<usize>;
    /// Find the end of a line, following line wraps.
    fn line_search_right(&self, _: Point<usize>) -> Point<usize>;
    /// Find the nearest matching bracket.
    fn bracket_search(&self, _: Point<usize>) -> Option<Point<usize>>;
}

impl<T> Search for Term<T> {
    fn semantic_search_left(&self, mut point: Point<usize>) -> Point<usize> {
        // Limit the starting point to the last line in the history
        point.line = min(point.line, self.grid.len() - 1);

        let mut iter = self.grid.iter_from(point);
        let last_col = self.grid.num_cols() - Column(1);

        while let Some(cell) = iter.prev() {
            if !cell.flags.intersects(Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER)
                && self.semantic_escape_chars.contains(cell.c)
            {
                break;
            }

            if iter.point().col == last_col && !cell.flags.contains(Flags::WRAPLINE) {
                break; // cut off if on new line or hit escape char
            }

            point = iter.point();
        }

        point
    }

    fn semantic_search_right(&self, mut point: Point<usize>) -> Point<usize> {
        // Limit the starting point to the last line in the history
        point.line = min(point.line, self.grid.len() - 1);

        let mut iter = self.grid.iter_from(point);
        let last_col = self.grid.num_cols() - 1;

        while let Some(cell) = iter.next() {
            if !cell.flags.intersects(Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER)
                && self.semantic_escape_chars.contains(cell.c)
            {
                break;
            }

            point = iter.point();

            if point.col == last_col && !cell.flags.contains(Flags::WRAPLINE) {
                break; // cut off if on new line or hit escape char
            }
        }

        point
    }

    fn line_search_left(&self, mut point: Point<usize>) -> Point<usize> {
        while point.line + 1 < self.grid.len()
            && self.grid[point.line + 1][self.grid.num_cols() - 1].flags.contains(Flags::WRAPLINE)
        {
            point.line += 1;
        }

        point.col = Column(0);

        point
    }

    fn line_search_right(&self, mut point: Point<usize>) -> Point<usize> {
        while self.grid[point.line][self.grid.num_cols() - 1].flags.contains(Flags::WRAPLINE) {
            point.line -= 1;
        }

        point.col = self.grid.num_cols() - 1;

        point
    }

    fn bracket_search(&self, point: Point<usize>) -> Option<Point<usize>> {
        let start_char = self.grid[point.line][point.col].c;

        // Find the matching bracket we're looking for
        let (forwards, end_char) = BRACKET_PAIRS.iter().find_map(|(open, close)| {
            if open == &start_char {
                Some((true, *close))
            } else if close == &start_char {
                Some((false, *open))
            } else {
                None
            }
        })?;

        let mut iter = self.grid.iter_from(point);

        // For every character match that equals the starting bracket, we
        // ignore one bracket of the opposite type.
        let mut skip_pairs = 0;

        loop {
            // Check the next cell
            let cell = if forwards { iter.next() } else { iter.prev() };

            // Break if there are no more cells
            let c = match cell {
                Some(cell) => cell.c,
                None => break,
            };

            // Check if the bracket matches
            if c == end_char && skip_pairs == 0 {
                return Some(iter.point());
            } else if c == start_char {
                skip_pairs += 1;
            } else if c == end_char {
                skip_pairs -= 1;
            }
        }

        None
    }
}

/// A key for caching cursor glyphs
#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash, Deserialize)]
pub struct CursorKey {
    pub style: CursorStyle,
    pub is_wide: bool,
}

/// Iterator that yields cells needing render
///
/// Yields cells that require work to be displayed (that is, not a an empty
/// background cell). Additionally, this manages some state of the grid only
/// relevant for rendering like temporarily changing the cell with the cursor.
///
/// This manages the cursor during a render. The cursor location is inverted to
/// draw it, and reverted after drawing to maintain state.
pub struct RenderableCellsIter<'a, C> {
    inner: DisplayIter<'a, Cell>,
    grid: &'a Grid<Cell>,
    cursor: &'a Point,
    cursor_offset: usize,
    cursor_key: Option<CursorKey>,
    cursor_style: CursorStyle,
    config: &'a Config<C>,
    colors: &'a color::List,
    selection: Option<SelectionRange<Line>>,
}

impl<'a, C> RenderableCellsIter<'a, C> {
    /// Create the renderable cells iterator
    ///
    /// The cursor and terminal mode are required for properly displaying the
    /// cursor.
    fn new<'b, T>(
        term: &'b Term<T>,
        config: &'b Config<C>,
        selection: Option<SelectionRange>,
        mut cursor_style: CursorStyle,
    ) -> RenderableCellsIter<'b, C> {
        let grid = &term.grid;
        let num_cols = grid.num_cols();
        let num_lines = grid.num_lines();

        let cursor_offset = grid.line_to_offset(term.cursor.point.line);
        let inner = grid.display_iter();

        let selection_range = selection.and_then(|span| {
            let (limit_start, limit_end) = if span.is_block {
                (span.start.col, span.end.col)
            } else {
                (Column(0), num_cols - 1)
            };

            // Get on-screen lines of the selection's locations
            let start = term.buffer_to_visible(span.start);
            let end = term.buffer_to_visible(span.end);

            // Clamp visible selection to the viewport
            let (mut start, mut end) = match (start, end) {
                (Some(start), Some(end)) => (start, end),
                (Some(start), None) => {
                    let end = Point::new(num_lines.0 - 1, num_cols - 1);
                    (start, end)
                },
                (None, Some(end)) => {
                    let start = Point::new(0, Column(0));
                    (start, end)
                },
                (None, None) => return None,
            };

            // Trim start/end with partially visible block selection
            start.col = max(limit_start, start.col);
            end.col = min(limit_end, end.col);

            Some(SelectionRange::new(start.into(), end.into(), span.is_block))
        });

        // Load cursor glyph
        let cursor = &term.cursor.point;
        let cursor_visible = term.mode.contains(TermMode::SHOW_CURSOR) && grid.contains(cursor);
        let cursor_key = if cursor_visible {
            let is_wide =
                grid[cursor].flags.contains(Flags::WIDE_CHAR) && (cursor.col + 1) < num_cols;
            Some(CursorKey { style: cursor_style, is_wide })
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
            config,
            colors: &term.colors,
            cursor_key,
            cursor_style,
        }
    }

    /// Check selection state of a cell.
    fn is_selected(&self, point: Point) -> bool {
        let selection = match self.selection {
            Some(selection) => selection,
            None => return false,
        };

        // Point itself is selected
        if selection.contains(point.col, point.line) {
            return true;
        }

        let num_cols = self.grid.num_cols().0;
        let cell = self.grid[&point];

        // Check if wide char's spacers are selected
        if cell.flags.contains(Flags::WIDE_CHAR) {
            let prevprev = point.sub(num_cols, 2);
            let prev = point.sub(num_cols, 1);
            let next = point.add(num_cols, 1);

            // Check trailing spacer
            selection.contains(next.col, next.line)
                // Check line-wrapping, leading spacer
                || (self.grid[&prev].flags.contains(Flags::WIDE_CHAR_SPACER)
                    && !self.grid[&prevprev].flags.contains(Flags::WIDE_CHAR)
                    && selection.contains(prev.col, prev.line))
        } else if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            // Check if spacer's wide char is selected
            let prev = point.sub(num_cols, 1);

            if self.grid[&prev].flags.contains(Flags::WIDE_CHAR) {
                // Check previous cell for trailing spacer
                self.is_selected(prev)
            } else {
                // Check next cell for line-wrapping, leading spacer
                self.is_selected(point.add(num_cols, 1))
            }
        } else {
            false
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum RenderableCellContent {
    Chars([char; cell::MAX_ZEROWIDTH_CHARS + 1]),
    Cursor(CursorKey),
}

#[derive(Copy, Clone, Debug)]
pub struct RenderableCell {
    /// A _Display_ line (not necessarily an _Active_ line)
    pub line: Line,
    pub column: Column,
    pub inner: RenderableCellContent,
    pub fg: Rgb,
    pub bg: Rgb,
    pub bg_alpha: f32,
    pub flags: Flags,
}

impl RenderableCell {
    fn new<C>(
        config: &Config<C>,
        colors: &color::List,
        cell: Indexed<Cell>,
        selected: bool,
    ) -> Self {
        // Lookup RGB values
        let mut fg_rgb = Self::compute_fg_rgb(config, colors, cell.fg, cell.flags);
        let mut bg_rgb = Self::compute_bg_rgb(colors, cell.bg);
        let mut bg_alpha = Self::compute_bg_alpha(cell.bg);

        let selection_background = config.colors.selection.background;
        if let (true, Some(col)) = (selected, selection_background) {
            // Override selection background with config colors
            bg_rgb = col;
            bg_alpha = 1.0;
        } else if selected ^ cell.inverse() {
            if fg_rgb == bg_rgb && !cell.flags.contains(Flags::HIDDEN) {
                // Reveal inversed text when fg/bg is the same
                fg_rgb = colors[NamedColor::Background];
                bg_rgb = colors[NamedColor::Foreground];
            } else {
                // Invert cell fg and bg colors
                mem::swap(&mut fg_rgb, &mut bg_rgb);
            }

            bg_alpha = 1.0;
        }

        // Override selection text with config colors
        if let (true, Some(col)) = (selected, config.colors.selection.text) {
            fg_rgb = col;
        }

        RenderableCell {
            line: cell.line,
            column: cell.column,
            inner: RenderableCellContent::Chars(cell.chars()),
            fg: fg_rgb,
            bg: bg_rgb,
            bg_alpha,
            flags: cell.flags,
        }
    }

    fn compute_fg_rgb<C>(config: &Config<C>, colors: &color::List, fg: Color, flags: Flags) -> Rgb {
        match fg {
            Color::Spec(rgb) => rgb,
            Color::Named(ansi) => {
                match (config.draw_bold_text_with_bright_colors(), flags & Flags::DIM_BOLD) {
                    // If no bright foreground is set, treat it like the BOLD flag doesn't exist
                    (_, Flags::DIM_BOLD)
                        if ansi == NamedColor::Foreground
                            && config.colors.primary.bright_foreground.is_none() =>
                    {
                        colors[NamedColor::DimForeground]
                    },
                    // Draw bold text in bright colors *and* contains bold flag.
                    (true, Flags::BOLD) => colors[ansi.to_bright()],
                    // Cell is marked as dim and not bold
                    (_, Flags::DIM) | (false, Flags::DIM_BOLD) => colors[ansi.to_dim()],
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
                    (true, Flags::BOLD, 0..=7) => idx as usize + 8,
                    (false, Flags::DIM, 8..=15) => idx as usize - 8,
                    (false, Flags::DIM, 0..=7) => idx as usize + 260,
                    _ => idx as usize,
                };

                colors[idx]
            },
        }
    }

    #[inline]
    fn compute_bg_alpha(bg: Color) -> f32 {
        if bg == Color::Named(NamedColor::Background) {
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

impl<'a, C> Iterator for RenderableCellsIter<'a, C> {
    type Item = RenderableCell;

    /// Gets the next renderable cell
    ///
    /// Skips empty (background) cells and applies any flags to the cell state
    /// (eg. invert fg and bg colors).
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.cursor_offset == self.inner.offset() && self.inner.column() == self.cursor.col {
                let selected = self.is_selected(Point::new(self.cursor.line, self.cursor.col));

                // Handle cursor
                if let Some(cursor_key) = self.cursor_key.take() {
                    let cell = Indexed {
                        inner: self.grid[self.cursor],
                        column: self.cursor.col,
                        // Using `self.cursor.line` leads to inconsitent cursor position when
                        // scrolling. See https://github.com/alacritty/alacritty/issues/2570 for more
                        // info.
                        line: self.inner.line(),
                    };

                    let mut renderable_cell =
                        RenderableCell::new(self.config, self.colors, cell, selected);

                    renderable_cell.inner = RenderableCellContent::Cursor(cursor_key);

                    if let Some(color) = self.config.cursor_cursor_color() {
                        renderable_cell.fg = RenderableCell::compute_bg_rgb(self.colors, color);
                    }

                    return Some(renderable_cell);
                } else {
                    let mut cell =
                        RenderableCell::new(self.config, self.colors, self.inner.next()?, selected);

                    if self.cursor_style == CursorStyle::Block {
                        std::mem::swap(&mut cell.bg, &mut cell.fg);

                        if let Some(color) = self.config.cursor_text_color() {
                            cell.fg = color;
                        }
                    }

                    return Some(cell);
                }
            } else {
                let cell = self.inner.next()?;

                let selected = self.is_selected(Point::new(cell.line, cell.column));

                if !cell.is_empty() || selected {
                    return Some(RenderableCell::new(self.config, self.colors, cell, selected));
                }
            }
        }
    }
}

pub mod mode {
    use bitflags::bitflags;

    bitflags! {
        pub struct TermMode: u16 {
            const SHOW_CURSOR         = 0b0000_0000_0000_0001;
            const APP_CURSOR          = 0b0000_0000_0000_0010;
            const APP_KEYPAD          = 0b0000_0000_0000_0100;
            const MOUSE_REPORT_CLICK  = 0b0000_0000_0000_1000;
            const BRACKETED_PASTE     = 0b0000_0000_0001_0000;
            const SGR_MOUSE           = 0b0000_0000_0010_0000;
            const MOUSE_MOTION        = 0b0000_0000_0100_0000;
            const LINE_WRAP           = 0b0000_0000_1000_0000;
            const LINE_FEED_NEW_LINE  = 0b0000_0001_0000_0000;
            const ORIGIN              = 0b0000_0010_0000_0000;
            const INSERT              = 0b0000_0100_0000_0000;
            const FOCUS_IN_OUT        = 0b0000_1000_0000_0000;
            const ALT_SCREEN          = 0b0001_0000_0000_0000;
            const MOUSE_DRAG          = 0b0010_0000_0000_0000;
            const MOUSE_MODE          = 0b0010_0000_0100_1000;
            const UTF8_MOUSE          = 0b0100_0000_0000_0000;
            const ALTERNATE_SCROLL    = 0b1000_0000_0000_0000;
            const ANY                 = 0b1111_1111_1111_1111;
            const NONE                = 0;
        }
    }

    impl Default for TermMode {
        fn default() -> TermMode {
            TermMode::SHOW_CURSOR | TermMode::LINE_WRAP | TermMode::ALTERNATE_SCROLL
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
    pub fn new<C>(config: &Config<C>) -> VisualBell {
        let visual_bell_config = &config.visual_bell;
        VisualBell {
            animation: visual_bell_config.animation,
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

    pub fn update_config<C>(&mut self, config: &Config<C>) {
        let visual_bell_config = &config.visual_bell;
        self.animation = visual_bell_config.animation;
        self.duration = visual_bell_config.duration();
    }
}

pub struct Term<T> {
    /// Terminal focus
    pub is_focused: bool,

    /// The grid
    grid: Grid<Cell>,

    /// Tracks if the next call to input will need to first handle wrapping.
    /// This is true after the last column is set with the input function. Any function that
    /// implicitly sets the line or column needs to set this to false to avoid wrapping twice.
    /// input_needs_wrap ensures that cursor.col is always valid for use into indexing into
    /// arrays. Without it we would have to sanitize cursor.col every time we used it.
    input_needs_wrap: bool,

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

    /// Scroll region.
    ///
    /// Range going from top to bottom of the terminal, indexed from the top of the viewport.
    scroll_region: Range<Line>,

    pub dirty: bool,

    pub visual_bell: VisualBell,

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

    /// Clipboard access coupled to the active window
    clipboard: Clipboard,

    /// Proxy for sending events to the event loop
    event_proxy: T,

    /// Current title of the window
    title: String,

    /// Stack of saved window titles. When a title is popped from this stack, the `title` for the
    /// term is set, and the Glutin window's title attribute is changed through the event listener.
    title_stack: Vec<String>,
}

/// Terminal size info
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq)]
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

    /// Check if coordinates are inside the terminal grid.
    ///
    /// The padding is not counted as part of the grid.
    pub fn contains_point(&self, x: usize, y: usize) -> bool {
        x < (self.width - self.padding_x) as usize
            && x >= self.padding_x as usize
            && y < (self.height - self.padding_y) as usize
            && y >= self.padding_y as usize
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

impl<T> Term<T> {
    pub fn selection(&self) -> &Option<Selection> {
        &self.grid.selection
    }

    pub fn selection_mut(&mut self) -> &mut Option<Selection> {
        &mut self.grid.selection
    }

    #[inline]
    pub fn scroll_display(&mut self, scroll: Scroll)
    where
        T: EventListener,
    {
        self.event_proxy.send_event(Event::MouseCursorDirty);
        self.grid.scroll_display(scroll);
        self.dirty = true;
    }

    pub fn new<C>(
        config: &Config<C>,
        size: &SizeInfo,
        clipboard: Clipboard,
        event_proxy: T,
    ) -> Term<T> {
        let num_cols = size.cols();
        let num_lines = size.lines();

        let history_size = config.scrolling.history() as usize;
        let grid = Grid::new(num_lines, num_cols, history_size, Cell::default());
        let alt = Grid::new(num_lines, num_cols, 0 /* scroll history */, Cell::default());

        let tabspaces = config.tabspaces();
        let tabs = TabStops::new(grid.num_cols(), tabspaces);

        let scroll_region = Line(0)..grid.num_lines();

        let colors = color::List::from(&config.colors);

        Term {
            dirty: false,
            visual_bell: VisualBell::new(config),
            input_needs_wrap: false,
            grid,
            alt_grid: alt,
            alt: false,
            active_charset: Default::default(),
            cursor: Default::default(),
            cursor_save: Default::default(),
            cursor_save_alt: Default::default(),
            tabs,
            mode: Default::default(),
            scroll_region,
            colors,
            color_modified: [false; color::COUNT],
            original_colors: colors,
            semantic_escape_chars: config.selection.semantic_escape_chars().to_owned(),
            cursor_style: None,
            default_cursor_style: config.cursor.style,
            dynamic_title: config.dynamic_title(),
            tabspaces,
            clipboard,
            event_proxy,
            is_focused: true,
            title: config.window.title.clone(),
            title_stack: Vec::new(),
        }
    }

    pub fn update_config<C>(&mut self, config: &Config<C>) {
        self.semantic_escape_chars = config.selection.semantic_escape_chars().to_owned();
        self.original_colors.fill_named(&config.colors);
        self.original_colors.fill_cube(&config.colors);
        self.original_colors.fill_gray_ramp(&config.colors);
        for i in 0..color::COUNT {
            if !self.color_modified[i] {
                self.colors[i] = self.original_colors[i];
            }
        }
        self.visual_bell.update_config(config);
        if let Some(0) = config.scrolling.faux_multiplier() {
            self.mode.remove(TermMode::ALTERNATE_SCROLL);
        }
        self.default_cursor_style = config.cursor.style;
        self.dynamic_title = config.dynamic_title();
        self.grid.update_history(config.scrolling.history() as usize);
    }

    /// Convert the active selection to a String.
    pub fn selection_to_string(&self) -> Option<String> {
        let selection = self.grid.selection.clone()?;
        let SelectionRange { start, end, is_block } = selection.to_range(self)?;

        let mut res = String::new();

        if is_block {
            for line in (end.line + 1..=start.line).rev() {
                res += &self.line_to_string(line, start.col..end.col, start.col.0 != 0);

                // If the last column is included, newline is appended automatically
                if end.col != self.cols() - 1 {
                    res += "\n";
                }
            }
            res += &self.line_to_string(end.line, start.col..end.col, true);
        } else {
            res = self.bounds_to_string(start, end);
        }

        Some(res)
    }

    /// Convert range between two points to a String.
    pub fn bounds_to_string(&self, start: Point<usize>, end: Point<usize>) -> String {
        let mut res = String::new();

        for line in (end.line..=start.line).rev() {
            let start_col = if line == start.line { start.col } else { Column(0) };
            let end_col = if line == end.line { end.col } else { self.cols() - 1 };

            res += &self.line_to_string(line, start_col..end_col, line == end.line);
        }

        res
    }

    /// Convert a single line in the grid to a String.
    fn line_to_string(
        &self,
        line: usize,
        mut cols: Range<Column>,
        include_wrapped_wide: bool,
    ) -> String {
        let mut text = String::new();

        let grid_line = &self.grid[line];
        let line_length = min(grid_line.line_length(), cols.end + 1);

        // Include wide char when trailing spacer is selected
        if grid_line[cols.start].flags.contains(Flags::WIDE_CHAR_SPACER) {
            cols.start -= 1;
        }

        let mut tab_mode = false;
        for col in IndexRange::from(cols.start..line_length) {
            let cell = grid_line[col];

            // Skip over cells until next tab-stop once a tab was found
            if tab_mode {
                if self.tabs[col] {
                    tab_mode = false;
                } else {
                    continue;
                }
            }

            if cell.c == '\t' {
                tab_mode = true;
            }

            if !cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                // Push cells primary character
                text.push(cell.c);

                // Push zero-width characters
                for c in (&cell.chars()[1..]).iter().take_while(|c| **c != ' ') {
                    text.push(*c);
                }
            }
        }

        if cols.end >= self.cols() - 1
            && (line_length.0 == 0
                || !self.grid[line][line_length - 1].flags.contains(Flags::WRAPLINE))
        {
            text.push('\n');
        }

        // If wide char is not part of the selection, but leading spacer is, include it
        if line_length == self.grid.num_cols()
            && line_length.0 >= 2
            && grid_line[line_length - 1].flags.contains(Flags::WIDE_CHAR_SPACER)
            && !grid_line[line_length - 2].flags.contains(Flags::WIDE_CHAR)
            && include_wrapped_wide
        {
            text.push(self.grid[line - 1][Column(0)].c);
        }

        text
    }

    pub fn visible_to_buffer(&self, point: Point) -> Point<usize> {
        self.grid.visible_to_buffer(point)
    }

    pub fn buffer_to_visible(&self, point: impl Into<Point<usize>>) -> Option<Point<usize>> {
        self.grid.buffer_to_visible(point)
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
    pub fn renderable_cells<'b, C>(&'b self, config: &'b Config<C>) -> RenderableCellsIter<'_, C> {
        let selection = self.grid.selection.as_ref().and_then(|s| s.to_range(self));

        let cursor = if self.is_focused || !config.cursor.unfocused_hollow() {
            self.cursor_style.unwrap_or(self.default_cursor_style)
        } else {
            CursorStyle::HollowBlock
        };

        RenderableCellsIter::new(&self, config, selection, cursor)
    }

    /// Resize terminal to new dimensions
    pub fn resize(&mut self, size: &SizeInfo) {
        let old_cols = self.grid.num_cols();
        let old_lines = self.grid.num_lines();
        let mut num_cols = size.cols();
        let mut num_lines = size.lines();

        if old_cols == num_cols && old_lines == num_lines {
            debug!("Term::resize dimensions unchanged");
            return;
        }

        self.grid.selection = None;
        self.alt_grid.selection = None;

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
            let template = Cell { bg: self.cursor.template.bg, ..Cell::default() };
            self.grid.scroll_up(&(Line(0)..old_lines), lines, &template);
        }

        // Scroll up alt grid as well
        if self.cursor_save_alt.point.line >= num_lines {
            let lines = self.cursor_save_alt.point.line - num_lines + 1;
            let template = Cell { bg: self.cursor_save_alt.template.bg, ..Cell::default() };
            self.alt_grid.scroll_up(&(Line(0)..old_lines), lines, &template);
        }

        // Move prompt down when growing if scrollback lines are available
        if num_lines > old_lines && !self.mode.contains(TermMode::ALT_SCREEN) {
            let growage = min(num_lines - old_lines, Line(self.grid.history_size()));
            self.cursor.point.line += growage;
        }

        debug!("New num_cols is {} and num_lines is {}", num_cols, num_lines);

        // Resize grids to new size
        let is_alt = self.mode.contains(TermMode::ALT_SCREEN);
        let alt_cursor_point =
            if is_alt { &mut self.cursor_save.point } else { &mut self.cursor_save_alt.point };
        self.grid.resize(!is_alt, num_lines, num_cols, &mut self.cursor.point, &Cell::default());
        self.alt_grid.resize(is_alt, num_lines, num_cols, alt_cursor_point, &Cell::default());

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
    pub fn mode(&self) -> &TermMode {
        &self.mode
    }

    #[inline]
    pub fn cursor(&self) -> &Cursor {
        &self.cursor
    }

    pub fn swap_alt(&mut self) {
        if self.alt {
            let template = self.cursor.template;
            self.grid.region_mut(..).each(|c| c.reset(&template));
        }

        self.alt = !self.alt;
        std::mem::swap(&mut self.grid, &mut self.alt_grid);
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
        let template = Cell { bg: self.cursor.template.bg, ..Cell::default() };
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
        let template = Cell { bg: self.cursor.template.bg, ..Cell::default() };
        self.grid.scroll_up(&(origin..self.scroll_region.end), lines, &template);
    }

    fn deccolm(&mut self)
    where
        T: EventListener,
    {
        // Setting 132 column font makes no sense, but run the other side effects
        // Clear scrolling region
        self.set_scrolling_region(1, self.grid.num_lines().0);

        // Clear grid
        let template = self.cursor.template;
        self.grid.region_mut(..).each(|c| c.reset(&template));
    }

    #[inline]
    pub fn background_color(&self) -> Rgb {
        self.colors[NamedColor::Background]
    }

    #[inline]
    pub fn exit(&mut self)
    where
        T: EventListener,
    {
        self.event_proxy.send_event(Event::Exit);
    }

    pub fn clipboard(&mut self) -> &mut Clipboard {
        &mut self.clipboard
    }

    /// Insert a linebreak at the current cursor position.
    #[inline]
    fn wrapline(&mut self)
    where
        T: EventListener,
    {
        if !self.mode.contains(TermMode::LINE_WRAP) {
            return;
        }

        trace!("Wrapping input");

        self.grid[&self.cursor.point].flags.insert(Flags::WRAPLINE);

        if (self.cursor.point.line + 1) >= self.scroll_region.end {
            self.linefeed();
        } else {
            self.cursor.point.line += 1;
        }

        self.cursor.point.col = Column(0);
        self.input_needs_wrap = false;
    }

    /// Write `c` to the cell at the cursor position.
    #[inline]
    fn write_at_cursor(&mut self, c: char) -> &mut Cell
    where
        T: EventListener,
    {
        let cell = &mut self.grid[&self.cursor.point];
        *cell = self.cursor.template;
        cell.c = self.cursor.charsets[self.active_charset].map(c);
        cell
    }
}

impl<T> TermInfo for Term<T> {
    #[inline]
    fn lines(&self) -> Line {
        self.grid.num_lines()
    }

    #[inline]
    fn cols(&self) -> Column {
        self.grid.num_cols()
    }
}

impl<T: EventListener> Handler for Term<T> {
    #[inline]
    #[cfg(not(windows))]
    fn set_title(&mut self, title: &str) {
        if self.dynamic_title {
            trace!("Setting window title to '{}'", title);

            self.title = title.into();
            self.event_proxy.send_event(Event::Title(title.to_owned()));
        }
    }

    #[inline]
    #[cfg(windows)]
    fn set_title(&mut self, title: &str) {
        if self.dynamic_title {
            // cmd.exe in winpty: winpty incorrectly sets the title to ' ' instead of
            // 'Alacritty' - thus we have to substitute this back to get equivalent
            // behaviour as conpty.
            //
            // The starts_with check is necessary because other shells e.g. bash set a
            // different title and don't need Alacritty prepended.
            trace!("Setting window title to '{}'", title);

            let title = if !tty::is_conpty() && title.starts_with(' ') {
                format!("Alacritty {}", title.trim())
            } else {
                title.to_owned()
            };

            self.title = title.clone();
            self.event_proxy.send_event(Event::Title(title));
        }
    }

    /// A character to be displayed
    #[inline]
    fn input(&mut self, c: char) {
        // Number of cells the char will occupy
        let width = match c.width() {
            Some(width) => width,
            None => return,
        };

        // Handle zero-width characters
        if width == 0 {
            let mut col = self.cursor.point.col.0.saturating_sub(1);
            let line = self.cursor.point.line;
            if self.grid[line][Column(col)].flags.contains(Flags::WIDE_CHAR_SPACER) {
                col = col.saturating_sub(1);
            }
            self.grid[line][Column(col)].push_extra(c);
            return;
        }

        // Move cursor to next line
        if self.input_needs_wrap {
            self.wrapline();
        }

        let num_cols = self.grid.num_cols();

        // If in insert mode, first shift cells to the right
        if self.mode.contains(TermMode::INSERT) && self.cursor.point.col + width < num_cols {
            let line = self.cursor.point.line;
            let col = self.cursor.point.col;
            let line = &mut self.grid[line];

            let src = line[col..].as_ptr();
            let dst = line[(col + width)..].as_mut_ptr();
            unsafe {
                ptr::copy(src, dst, (num_cols - col - width).0);
            }
        }

        if width == 1 {
            self.write_at_cursor(c);
        } else {
            // Insert extra placeholder before wide char if glyph doesn't fit in this row anymore
            if self.cursor.point.col + 1 >= num_cols {
                self.write_at_cursor(' ').flags.insert(Flags::WIDE_CHAR_SPACER);
                self.wrapline();
            }

            // Write full width glyph to current cursor cell
            self.write_at_cursor(c).flags.insert(Flags::WIDE_CHAR);

            // Write spacer to cell following the wide glyph
            self.cursor.point.col += 1;
            self.write_at_cursor(' ').flags.insert(Flags::WIDE_CHAR_SPACER);
        }

        if self.cursor.point.col + 1 < num_cols {
            self.cursor.point.col += 1;
        } else {
            self.input_needs_wrap = true;
        }
    }

    #[inline]
    fn decaln(&mut self) {
        trace!("Decalnning");

        let template = Cell { c: 'E', ..Cell::default() };
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

        let count = min(count, self.grid.num_cols() - self.cursor.point.col);

        let source = self.cursor.point.col;
        let destination = self.cursor.point.col + count;
        let num_cells = (self.grid.num_cols() - destination).0;

        let line = &mut self.grid[self.cursor.point.line];

        unsafe {
            let src = line[source..].as_ptr();
            let dst = line[destination..].as_mut_ptr();

            ptr::copy(src, dst, num_cells);
        }

        // Cells were just moved out towards the end of the line; fill in
        // between source and dest with blanks.
        for c in &mut line[source..destination] {
            c.reset(&self.cursor.template);
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
        trace!("Reporting terminal identity");
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
                let response = format!("\x1b[{};{}R", pos.line + 1, pos.col + 1);
                let _ = writer.write_all(response.as_bytes());
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
        self.event_proxy.send_event(Event::Urgent);
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
        if self.scroll_region.contains(&self.cursor.point.line) {
            let origin = self.cursor.point.line;
            self.scroll_down_relative(origin, lines);
        }
    }

    #[inline]
    fn delete_lines(&mut self, lines: Line) {
        let origin = self.cursor.point.line;
        let lines = min(self.lines() - origin, lines);

        trace!("Deleting {} lines", lines);

        if lines.0 > 0 && self.scroll_region.contains(&self.cursor.point.line) {
            self.scroll_up_relative(origin, lines);
        }
    }

    #[inline]
    fn erase_chars(&mut self, count: Column) {
        trace!("Erasing chars: count={}, col={}", count, self.cursor.point.col);
        let start = self.cursor.point.col;
        let end = min(start + count, self.grid.num_cols());

        let row = &mut self.grid[self.cursor.point.line];
        // Cleared cells have current background color set
        for c in &mut row[start..end] {
            c.reset(&self.cursor.template);
        }
    }

    #[inline]
    fn delete_chars(&mut self, count: Column) {
        let cols = self.grid.num_cols();

        // Ensure deleting within terminal bounds
        let count = min(count, cols);

        let start = self.cursor.point.col;
        let end = min(start + count, cols - 1);
        let n = (cols - end).0;

        let line = &mut self.grid[self.cursor.point.line];

        unsafe {
            let src = line[end..].as_ptr();
            let dst = line[start..].as_mut_ptr();

            ptr::copy(src, dst, n);
        }

        // Clear last `count` cells in line. If deleting 1 char, need to delete
        // 1 cell.
        let end = cols - count;
        for c in &mut line[end..] {
            c.reset(&self.cursor.template);
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

        let col = self.cursor.point.col;

        match mode {
            ansi::LineClearMode::Right => {
                let row = &mut self.grid[self.cursor.point.line];
                for cell in &mut row[col..] {
                    cell.reset(&self.cursor.template);
                }
            },
            ansi::LineClearMode::Left => {
                let row = &mut self.grid[self.cursor.point.line];
                for cell in &mut row[..=col] {
                    cell.reset(&self.cursor.template);
                }
            },
            ansi::LineClearMode::All => {
                let row = &mut self.grid[self.cursor.point.line];
                for cell in &mut row[..] {
                    cell.reset(&self.cursor.template);
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

    /// Write a foreground/background color escape sequence with the current color
    #[inline]
    fn dynamic_color_sequence<W: io::Write>(
        &mut self,
        writer: &mut W,
        code: u8,
        index: usize,
        terminator: &str,
    ) {
        trace!("Writing escape sequence for dynamic color code {}: color[{}]", code, index);
        let color = self.colors[index];
        let response = format!(
            "\x1b]{};rgb:{1:02x}{1:02x}/{2:02x}{2:02x}/{3:02x}{3:02x}{4}",
            code, color.r, color.g, color.b, terminator
        );
        let _ = writer.write_all(response.as_bytes());
    }

    /// Reset the indexed color to original value
    #[inline]
    fn reset_color(&mut self, index: usize) {
        trace!("Resetting color[{}]", index);
        self.colors[index] = self.original_colors[index];
        self.color_modified[index] = false;
    }

    /// Set the clipboard
    #[inline]
    fn set_clipboard(&mut self, clipboard: u8, base64: &[u8]) {
        let clipboard_type = match clipboard {
            b'c' => ClipboardType::Clipboard,
            b'p' | b's' => ClipboardType::Selection,
            _ => return,
        };

        if let Ok(bytes) = base64::decode(base64) {
            if let Ok(text) = str::from_utf8(&bytes) {
                self.clipboard.store(clipboard_type, text);
            }
        }
    }

    /// Write clipboard data to child.
    #[inline]
    fn write_clipboard<W: io::Write>(&mut self, clipboard: u8, writer: &mut W, terminator: &str) {
        let clipboard_type = match clipboard {
            b'c' => ClipboardType::Clipboard,
            b'p' | b's' => ClipboardType::Selection,
            _ => return,
        };

        let text = self.clipboard.load(clipboard_type);
        let base64 = base64::encode(&text);
        let escape = format!("\x1b]52;{};{}{}", clipboard as char, base64, terminator);
        let _ = writer.write_all(escape.as_bytes());
    }

    #[inline]
    fn clear_screen(&mut self, mode: ansi::ClearMode) {
        trace!("Clearing screen: {:?}", mode);
        let template = self.cursor.template;

        // Remove active selections
        self.grid.selection = None;

        match mode {
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
            ansi::ClearMode::All => {
                if self.mode.contains(TermMode::ALT_SCREEN) {
                    self.grid.region_mut(..).each(|c| c.reset(&template));
                } else {
                    let template = Cell { bg: template.bg, ..Cell::default() };
                    self.grid.clear_viewport(&template);
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
        self.cursor = Default::default();
        self.active_charset = Default::default();
        self.mode = Default::default();
        self.cursor_save = Default::default();
        self.cursor_save_alt = Default::default();
        self.colors = self.original_colors;
        self.color_modified = [false; color::COUNT];
        self.cursor_style = None;
        self.grid.reset(&Cell::default());
        self.alt_grid.reset(&Cell::default());
        self.scroll_region = Line(0)..self.grid.num_lines();
        self.title = DEFAULT_NAME.to_string();
        self.title_stack.clear();
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
                self.cursor.template.flags = Flags::empty();
            },
            Attr::Reverse => self.cursor.template.flags.insert(Flags::INVERSE),
            Attr::CancelReverse => self.cursor.template.flags.remove(Flags::INVERSE),
            Attr::Bold => self.cursor.template.flags.insert(Flags::BOLD),
            Attr::CancelBold => self.cursor.template.flags.remove(Flags::BOLD),
            Attr::Dim => self.cursor.template.flags.insert(Flags::DIM),
            Attr::CancelBoldDim => self.cursor.template.flags.remove(Flags::BOLD | Flags::DIM),
            Attr::Italic => self.cursor.template.flags.insert(Flags::ITALIC),
            Attr::CancelItalic => self.cursor.template.flags.remove(Flags::ITALIC),
            Attr::Underline => self.cursor.template.flags.insert(Flags::UNDERLINE),
            Attr::CancelUnderline => self.cursor.template.flags.remove(Flags::UNDERLINE),
            Attr::Hidden => self.cursor.template.flags.insert(Flags::HIDDEN),
            Attr::CancelHidden => self.cursor.template.flags.remove(Flags::HIDDEN),
            Attr::Strike => self.cursor.template.flags.insert(Flags::STRIKEOUT),
            Attr::CancelStrike => self.cursor.template.flags.remove(Flags::STRIKEOUT),
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
            // Mouse protocols are mutually exlusive
            ansi::Mode::ReportMouseClicks => {
                self.mode.remove(TermMode::MOUSE_MODE);
                self.mode.insert(TermMode::MOUSE_REPORT_CLICK);
                self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            ansi::Mode::ReportCellMouseMotion => {
                self.mode.remove(TermMode::MOUSE_MODE);
                self.mode.insert(TermMode::MOUSE_DRAG);
                self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            ansi::Mode::ReportAllMouseMotion => {
                self.mode.remove(TermMode::MOUSE_MODE);
                self.mode.insert(TermMode::MOUSE_MOTION);
                self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            ansi::Mode::ReportFocusInOut => self.mode.insert(TermMode::FOCUS_IN_OUT),
            ansi::Mode::BracketedPaste => self.mode.insert(TermMode::BRACKETED_PASTE),
            // Mouse encodings are mutually exlusive
            ansi::Mode::SgrMouse => {
                self.mode.remove(TermMode::UTF8_MOUSE);
                self.mode.insert(TermMode::SGR_MOUSE);
            },
            ansi::Mode::Utf8Mouse => {
                self.mode.remove(TermMode::SGR_MOUSE);
                self.mode.insert(TermMode::UTF8_MOUSE);
            },
            ansi::Mode::AlternateScroll => self.mode.insert(TermMode::ALTERNATE_SCROLL),
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
                self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            ansi::Mode::ReportCellMouseMotion => {
                self.mode.remove(TermMode::MOUSE_DRAG);
                self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            ansi::Mode::ReportAllMouseMotion => {
                self.mode.remove(TermMode::MOUSE_MOTION);
                self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            ansi::Mode::ReportFocusInOut => self.mode.remove(TermMode::FOCUS_IN_OUT),
            ansi::Mode::BracketedPaste => self.mode.remove(TermMode::BRACKETED_PASTE),
            ansi::Mode::SgrMouse => self.mode.remove(TermMode::SGR_MOUSE),
            ansi::Mode::Utf8Mouse => self.mode.remove(TermMode::UTF8_MOUSE),
            ansi::Mode::AlternateScroll => self.mode.remove(TermMode::ALTERNATE_SCROLL),
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
    fn set_scrolling_region(&mut self, top: usize, bottom: usize) {
        if top >= bottom {
            debug!("Invalid scrolling region: ({};{})", top, bottom);
            return;
        }

        // Bottom should be included in the range, but range end is not
        // usually included. One option would be to use an inclusive
        // range, but instead we just let the open range end be 1
        // higher.
        let start = Line(top - 1);
        let end = Line(bottom);

        trace!("Setting scrolling region: ({};{})", start, end);

        self.scroll_region.start = min(start, self.grid.num_lines());
        self.scroll_region.end = min(end, self.grid.num_lines());
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

    #[inline]
    fn push_title(&mut self) {
        trace!("Pushing '{}' onto title stack", self.title);

        if self.title_stack.len() >= TITLE_STACK_MAX_DEPTH {
            let removed = self.title_stack.remove(0);
            trace!(
                "Removing '{}' from bottom of title stack that exceeds its maximum depth",
                removed
            );
        }

        self.title_stack.push(self.title.clone());
    }

    #[inline]
    fn pop_title(&mut self) {
        trace!("Attempting to pop title from stack...");

        if let Some(popped) = self.title_stack.pop() {
            trace!("Title '{}' popped from stack", popped);
            self.set_title(&popped);
        }
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
    use std::mem;

    use crate::ansi::{self, CharsetIndex, Handler, StandardCharset};
    use crate::clipboard::Clipboard;
    use crate::config::MockConfig;
    use crate::event::{Event, EventListener};
    use crate::grid::{Grid, Scroll};
    use crate::index::{Column, Line, Point, Side};
    use crate::selection::Selection;
    use crate::term::cell::{Cell, Flags};
    use crate::term::{SizeInfo, Term};

    struct Mock;
    impl EventListener for Mock {
        fn send_event(&self, _event: Event) {}
    }

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
        let mut term = Term::new(&MockConfig::default(), &size, Clipboard::new_nop(), Mock);
        let mut grid: Grid<Cell> = Grid::new(Line(3), Column(5), 0, Cell::default());
        for i in 0..5 {
            for j in 0..2 {
                grid[Line(j)][Column(i)].c = 'a';
            }
        }
        grid[Line(0)][Column(0)].c = '"';
        grid[Line(0)][Column(3)].c = '"';
        grid[Line(1)][Column(2)].c = '"';
        grid[Line(0)][Column(4)].flags.insert(Flags::WRAPLINE);

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
        let mut term = Term::new(&MockConfig::default(), &size, Clipboard::new_nop(), Mock);
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
        let mut term = Term::new(&MockConfig::default(), &size, Clipboard::new_nop(), Mock);
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
        let mut term = Term::new(&MockConfig::default(), &size, Clipboard::new_nop(), Mock);
        let cursor = Point::new(Line(0), Column(0));
        term.configure_charset(CharsetIndex::G0, StandardCharset::SpecialCharacterAndLineDrawing);
        term.input('a');

        assert_eq!(term.grid()[&cursor].c, '▒');
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
        let mut term = Term::new(&MockConfig::default(), &size, Clipboard::new_nop(), Mock);

        // Add one line of scrollback
        term.grid.scroll_up(&(Line(0)..Line(1)), Line(1), &Cell::default());

        // Clear the history
        term.clear_screen(ansi::ClearMode::Saved);

        // Make sure that scrolling does not change the grid
        let mut scrolled_grid = term.grid.clone();
        scrolled_grid.scroll_display(Scroll::Top);

        // Truncate grids for comparison
        scrolled_grid.truncate();
        term.grid.truncate();

        assert_eq!(term.grid, scrolled_grid);
    }

    #[test]
    fn window_title() {
        let size = SizeInfo {
            width: 21.0,
            height: 51.0,
            cell_width: 3.0,
            cell_height: 3.0,
            padding_x: 0.0,
            padding_y: 0.0,
            dpr: 1.0,
        };
        let mut term = Term::new(&MockConfig::default(), &size, Clipboard::new_nop(), Mock);

        // Title can be set
        {
            term.title = "Test".to_string();
            assert_eq!(term.title, "Test");
        }

        // Title can be pushed onto stack
        {
            term.push_title();
            term.title = "Next".to_string();
            assert_eq!(term.title, "Next");
            assert_eq!(term.title_stack.get(0).unwrap(), "Test");
        }

        // Title can be popped from stack and set as the window title
        {
            term.pop_title();
            assert_eq!(term.title, "Test");
            assert!(term.title_stack.is_empty());
        }

        // Title stack doesn't grow infinitely
        {
            for _ in 0..4097 {
                term.push_title();
            }
            assert_eq!(term.title_stack.len(), 4096);
        }

        // Title and title stack reset when terminal state is reset
        {
            term.push_title();
            term.reset_state();
            assert_eq!(term.title, "Alacritty");
            assert!(term.title_stack.is_empty());
        }
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

    use crate::clipboard::Clipboard;
    use crate::config::MockConfig;
    use crate::event::{Event, EventListener};
    use crate::grid::Grid;

    use super::cell::Cell;
    use super::{SizeInfo, Term};

    struct Mock;
    impl EventListener for Mock {
        fn send_event(&self, _event: Event) {}
    }

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

        let config = MockConfig::default();

        let mut terminal = Term::new(&config, &size, Clipboard::new_nop(), Mock);
        mem::swap(&mut terminal.grid, &mut grid);

        b.iter(|| {
            let iter = terminal.renderable_cells(&config);
            for cell in iter {
                test::black_box(cell);
            }
        })
    }
}
