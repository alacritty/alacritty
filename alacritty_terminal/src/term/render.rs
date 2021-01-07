use std::cmp::max;
use std::iter;
use std::iter::Peekable;
use std::mem;
use std::ops::RangeInclusive;

use crate::ansi::{Color, CursorShape, NamedColor};
use crate::config::Config;
use crate::grid::{Dimensions, DisplayIter, Indexed};
use crate::index::{Column, Direction, Line, Point};
use crate::selection::SelectionRange;
use crate::term::cell::{Cell, Flags};
use crate::term::color::{self, CellRgb, Rgb, DIM_FACTOR};
use crate::term::search::RegexIter;
use crate::term::{Term, TermMode};

/// Minimum contrast between a fixed cursor color and the cell's background.
pub const MIN_CURSOR_CONTRAST: f64 = 1.5;

/// Maximum number of linewraps followed outside of the viewport during search highlighting.
const MAX_SEARCH_LINES: usize = 100;

/// Renderable terminal content.
///
/// This provides the terminal cursor and an iterator over all non-empty cells.
pub struct RenderableContent<'a, T, C> {
    term: &'a Term<T>,
    config: &'a Config<C>,
    display_iter: DisplayIter<'a, Cell>,
    selection: Option<SelectionRange<Line>>,
    search: RenderableSearch<'a>,
    cursor: Option<RenderableCursor>,
    cursor_shape: CursorShape,
    cursor_point: Point,
}

impl<'a, T, C> RenderableContent<'a, T, C> {
    pub fn new(term: &'a Term<T>, config: &'a Config<C>, show_cursor: bool) -> Self {
        // Cursor position.
        let vi_mode = term.mode.contains(TermMode::VI);
        let mut cursor_point = if vi_mode {
            term.vi_mode_cursor.point
        } else {
            let mut point = term.grid.cursor.point;
            point.line += term.grid.display_offset();
            point
        };

        // Cursor shape.
        let cursor_shape = if !show_cursor
            || (!term.mode.contains(TermMode::SHOW_CURSOR) && !vi_mode)
            || cursor_point.line >= term.screen_lines()
        {
            cursor_point.line = Line(0);
            CursorShape::Hidden
        } else if !term.is_focused && config.cursor.unfocused_hollow {
            CursorShape::HollowBlock
        } else {
            let cursor_style = term.cursor_style.unwrap_or(term.default_cursor_style);

            if vi_mode {
                term.vi_mode_cursor_style.unwrap_or(cursor_style).shape
            } else {
                cursor_style.shape
            }
        };

        Self {
            display_iter: term.grid.display_iter(),
            selection: term.visible_selection(),
            search: RenderableSearch::new(term),
            cursor: None,
            cursor_shape,
            cursor_point,
            config,
            term,
        }
    }

    /// Get the terminal cursor.
    pub fn cursor(mut self) -> Option<RenderableCursor> {
        // Drain the iterator to make sure the cursor is created.
        while self.next().is_some() && self.cursor.is_none() {}

        self.cursor
    }

    /// Assemble the information required to render the terminal cursor.
    ///
    /// This will return `None` when there is no cursor visible.
    fn renderable_cursor(&mut self, cell: &RenderableCell) -> Option<RenderableCursor> {
        if self.cursor_shape == CursorShape::Hidden {
            return None;
        }

        // Expand across wide cell when inside wide char or spacer.
        let is_wide = if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            self.cursor_point.col -= 1;
            true
        } else {
            cell.flags.contains(Flags::WIDE_CHAR)
        };

        // Cursor colors.
        let color = if self.term.mode.contains(TermMode::VI) {
            self.config.colors.vi_mode_cursor
        } else {
            self.config.colors.cursor
        };
        let mut cursor_color = if self.term.color_modified[NamedColor::Cursor as usize] {
            CellRgb::Rgb(self.term.colors[NamedColor::Cursor])
        } else {
            color.background
        };
        let mut text_color = color.foreground;

        // Invert the cursor if it has a fixed background close to the cell's background.
        if matches!(
            cursor_color,
            CellRgb::Rgb(color) if color.contrast(cell.bg) < MIN_CURSOR_CONTRAST
        ) {
            cursor_color = CellRgb::CellForeground;
            text_color = CellRgb::CellBackground;
        }

        // Convert from cell colors to RGB.
        let text_color = text_color.color(cell.fg, cell.bg);
        let cursor_color = cursor_color.color(cell.fg, cell.bg);

        Some(RenderableCursor {
            point: self.cursor_point,
            shape: self.cursor_shape,
            cursor_color,
            text_color,
            is_wide,
        })
    }
}

impl<'a, T, C> Iterator for RenderableContent<'a, T, C> {
    type Item = RenderableCell;

    /// Gets the next renderable cell.
    ///
    /// Skips empty (background) cells and applies any flags to the cell state
    /// (eg. invert fg and bg colors).
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.cursor_point == self.display_iter.point() {
                // Handle cell at cursor position.
                let cell = self.display_iter.next()?;
                let mut cell = RenderableCell::new(self, cell);

                // Store the cursor which should be rendered.
                self.cursor = self.renderable_cursor(&cell).map(|cursor| {
                    if cursor.shape == CursorShape::Block {
                        cell.fg = cursor.text_color;
                        cell.bg = cursor.cursor_color;

                        // Since we draw Block cursor by drawing cell below it with a proper color,
                        // we must adjust alpha to make it visible.
                        cell.bg_alpha = 1.;
                    }

                    cursor
                });

                return Some(cell);
            } else {
                // Handle non-cursor cells.
                let cell = self.display_iter.next()?;
                let cell = RenderableCell::new(self, cell);

                // Skip empty cells and wide char spacers.
                if !cell.is_empty() && !cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    return Some(cell);
                }
            }
        }
    }
}

/// Cell ready for rendering.
#[derive(Clone, Debug)]
pub struct RenderableCell {
    pub character: char,
    pub zerowidth: Option<Vec<char>>,
    pub line: Line,
    pub column: Column,
    pub fg: Rgb,
    pub bg: Rgb,
    pub bg_alpha: f32,
    pub flags: Flags,
    pub is_match: bool,
}

impl RenderableCell {
    fn new<'a, T, C>(content: &mut RenderableContent<'a, T, C>, cell: Indexed<&Cell>) -> Self {
        let point = Point::new(cell.line, cell.column);

        // Lookup RGB values.
        let mut fg_rgb =
            Self::compute_fg_rgb(content.config, &content.term.colors, cell.fg, cell.flags);
        let mut bg_rgb = Self::compute_bg_rgb(&content.term.colors, cell.bg);

        let mut bg_alpha = if cell.flags.contains(Flags::INVERSE) {
            mem::swap(&mut fg_rgb, &mut bg_rgb);
            1.0
        } else {
            Self::compute_bg_alpha(cell.bg)
        };

        let grid = content.term.grid();
        let is_selected = content.selection.map_or(false, |selection| {
            selection.contains_cell(grid, point, content.cursor_point, content.cursor_shape)
        });
        let mut is_match = false;

        if is_selected {
            let config_bg = content.config.colors.selection.background;
            let selected_fg = content.config.colors.selection.foreground.color(fg_rgb, bg_rgb);
            bg_rgb = config_bg.color(fg_rgb, bg_rgb);
            fg_rgb = selected_fg;

            if fg_rgb == bg_rgb && !cell.flags.contains(Flags::HIDDEN) {
                // Reveal inversed text when fg/bg is the same.
                fg_rgb = content.term.colors[NamedColor::Background];
                bg_rgb = content.term.colors[NamedColor::Foreground];
                bg_alpha = 1.0;
            } else if config_bg != CellRgb::CellBackground {
                bg_alpha = 1.0;
            }
        } else if content.search.advance(grid.visible_to_buffer(point)) {
            // Highlight the cell if it is part of a search match.
            let config_bg = content.config.colors.search.matches.background;
            let matched_fg = content.config.colors.search.matches.foreground.color(fg_rgb, bg_rgb);
            bg_rgb = config_bg.color(fg_rgb, bg_rgb);
            fg_rgb = matched_fg;

            if config_bg != CellRgb::CellBackground {
                bg_alpha = 1.0;
            }

            is_match = true;
        }

        RenderableCell {
            character: cell.c,
            zerowidth: cell.zerowidth().map(|zerowidth| zerowidth.to_vec()),
            line: cell.line,
            column: cell.column,
            fg: fg_rgb,
            bg: bg_rgb,
            bg_alpha,
            flags: cell.flags,
            is_match,
        }
    }

    /// Position of the cell.
    pub fn point(&self) -> Point {
        Point::new(self.line, self.column)
    }

    /// Check if cell contains any renderable content.
    fn is_empty(&self) -> bool {
        self.bg_alpha == 0.
            && !self.flags.intersects(Flags::UNDERLINE | Flags::STRIKEOUT | Flags::DOUBLE_UNDERLINE)
            && self.character == ' '
            && self.zerowidth.is_none()
    }

    /// Get the RGB color from a cell's foreground color.
    fn compute_fg_rgb<C>(config: &Config<C>, colors: &color::List, fg: Color, flags: Flags) -> Rgb {
        match fg {
            Color::Spec(rgb) => match flags & Flags::DIM {
                Flags::DIM => rgb * DIM_FACTOR,
                _ => rgb,
            },
            Color::Named(ansi) => {
                match (config.draw_bold_text_with_bright_colors, flags & Flags::DIM_BOLD) {
                    // If no bright foreground is set, treat it like the BOLD flag doesn't exist.
                    (_, Flags::DIM_BOLD)
                        if ansi == NamedColor::Foreground
                            && config.colors.primary.bright_foreground.is_none() =>
                    {
                        colors[NamedColor::DimForeground]
                    },
                    // Draw bold text in bright colors *and* contains bold flag.
                    (true, Flags::BOLD) => colors[ansi.to_bright()],
                    // Cell is marked as dim and not bold.
                    (_, Flags::DIM) | (false, Flags::DIM_BOLD) => colors[ansi.to_dim()],
                    // None of the above, keep original color..
                    _ => colors[ansi],
                }
            },
            Color::Indexed(idx) => {
                let idx = match (
                    config.draw_bold_text_with_bright_colors,
                    flags & Flags::DIM_BOLD,
                    idx,
                ) {
                    (true, Flags::BOLD, 0..=7) => idx as usize + 8,
                    (false, Flags::DIM, 8..=15) => idx as usize - 8,
                    (false, Flags::DIM, 0..=7) => NamedColor::DimBlack as usize + idx as usize,
                    _ => idx as usize,
                };

                colors[idx]
            },
        }
    }

    /// Get the RGB color from a cell's background color.
    #[inline]
    fn compute_bg_rgb(colors: &color::List, bg: Color) -> Rgb {
        match bg {
            Color::Spec(rgb) => rgb,
            Color::Named(ansi) => colors[ansi],
            Color::Indexed(idx) => colors[idx],
        }
    }

    /// Compute background alpha based on cell's original color.
    ///
    /// Since an RGB color matching the background should not be transparent, this is computed
    /// using the named input color, rather than checking the RGB of the background after its color
    /// is computed.
    #[inline]
    fn compute_bg_alpha(bg: Color) -> f32 {
        if bg == Color::Named(NamedColor::Background) {
            0.
        } else {
            1.
        }
    }
}

/// Cursor storing all information relevant for rendering.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub struct RenderableCursor {
    shape: CursorShape,
    cursor_color: Rgb,
    text_color: Rgb,
    is_wide: bool,
    point: Point,
}

impl RenderableCursor {
    pub fn color(&self) -> Rgb {
        self.cursor_color
    }

    pub fn shape(&self) -> CursorShape {
        self.shape
    }

    pub fn is_wide(&self) -> bool {
        self.is_wide
    }

    pub fn point(&self) -> Point {
        self.point
    }
}

type MatchIter<'a> = Box<dyn Iterator<Item = RangeInclusive<Point<usize>>> + 'a>;

/// Regex search highlight tracking.
struct RenderableSearch<'a> {
    iter: Peekable<MatchIter<'a>>,
}

impl<'a> RenderableSearch<'a> {
    /// Create a new renderable search iterator.
    fn new<T>(term: &'a Term<T>) -> Self {
        // Avoid constructing search if there is none.
        if term.regex_search.is_none() {
            let iter: MatchIter<'a> = Box::new(iter::empty());
            return Self { iter: iter.peekable() };
        }

        let viewport_end = term.grid().display_offset();
        let viewport_start = viewport_end + term.screen_lines().0 - 1;

        // Compute start of the first and end of the last line.
        let start_point = Point::new(viewport_start, Column(0));
        let mut start = term.line_search_left(start_point);
        let end_point = Point::new(viewport_end, term.cols() - 1);
        let mut end = term.line_search_right(end_point);

        // Set upper bound on search before/after the viewport to prevent excessive blocking.
        if start.line > viewport_start + MAX_SEARCH_LINES {
            if start.line == 0 {
                // Do not highlight anything if this line is the last.
                let iter: MatchIter<'a> = Box::new(iter::empty());
                return Self { iter: iter.peekable() };
            } else {
                // Start at next line if this one is too long.
                start.line -= 1;
            }
        }
        end.line = max(end.line, viewport_end.saturating_sub(MAX_SEARCH_LINES));

        // Create an iterater for the current regex search for all visible matches.
        let iter: MatchIter<'a> = Box::new(
            RegexIter::new(start, end, Direction::Right, &term)
                .skip_while(move |rm| rm.end().line > viewport_start)
                .take_while(move |rm| rm.start().line >= viewport_end),
        );

        Self { iter: iter.peekable() }
    }

    /// Advance the search tracker to the next point.
    ///
    /// This will return `true` if the point passed is part of a search match.
    fn advance(&mut self, point: Point<usize>) -> bool {
        while let Some(regex_match) = &self.iter.peek() {
            if regex_match.start() > &point {
                break;
            } else if regex_match.end() < &point {
                let _ = self.iter.next();
            } else {
                return true;
            }
        }
        false
    }
}
