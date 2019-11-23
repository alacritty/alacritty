use std::cmp::min;
use std::mem;

use glutin::event::{ElementState, ModifiersState};
use urlocator::{UrlLocation, UrlLocator};

use font::Metrics;

use alacritty_terminal::index::Point;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Rgb;
use alacritty_terminal::term::{RenderableCell, RenderableCellContent, SizeInfo};

use crate::config::{Config, RelaxedEq};
use crate::event::Mouse;
use crate::renderer::rects::{RenderLine, RenderRect};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Url {
    lines: Vec<RenderLine>,
    end_offset: u16,
    num_cols: usize,
}

impl Url {
    pub fn rects(&self, metrics: &Metrics, size: &SizeInfo) -> Vec<RenderRect> {
        let end = self.end();
        self.lines
            .iter()
            .filter(|line| line.start <= end)
            .map(|line| {
                let mut rect_line = *line;
                rect_line.end = min(line.end, end);
                rect_line.rects(Flags::UNDERLINE, metrics, size)
            })
            .flatten()
            .collect()
    }

    pub fn start(&self) -> Point {
        self.lines[0].start
    }

    pub fn end(&self) -> Point {
        self.lines[self.lines.len() - 1].end.sub(self.num_cols, self.end_offset as usize)
    }
}

pub struct Urls {
    locator: UrlLocator,
    urls: Vec<Url>,
    scheme_buffer: Vec<RenderableCell>,
    last_point: Option<Point>,
    state: UrlLocation,
}

impl Default for Urls {
    fn default() -> Self {
        Self {
            locator: UrlLocator::new(),
            scheme_buffer: Vec::new(),
            urls: Vec::new(),
            state: UrlLocation::Reset,
            last_point: None,
        }
    }
}

impl Urls {
    pub fn new() -> Self {
        Self::default()
    }

    // Update tracked URLs
    pub fn update(&mut self, num_cols: usize, cell: RenderableCell) {
        // Ignore double-width spacers to prevent reset
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            return;
        }

        // Convert cell to character
        let c = match cell.inner {
            RenderableCellContent::Chars(chars) => chars[0],
            RenderableCellContent::Cursor(_) => return,
        };

        let point: Point = cell.into();
        let mut end = point;

        // Reset URL when empty cells have been skipped
        if point != Point::default() && Some(point.sub(num_cols, 1)) != self.last_point {
            self.reset();
        }

        // Extend by one cell for double-width characters
        if cell.flags.contains(Flags::WIDE_CHAR) {
            end.col += 1;
        }

        self.last_point = Some(end);

        // Advance parser
        let last_state = mem::replace(&mut self.state, self.locator.advance(c));
        match (self.state, last_state) {
            (UrlLocation::Url(_length, end_offset), UrlLocation::Scheme) => {
                // Create empty URL
                self.urls.push(Url { lines: Vec::new(), end_offset, num_cols });

                // Push schemes into URL
                for scheme_cell in self.scheme_buffer.split_off(0) {
                    let point = scheme_cell.into();
                    self.extend_url(point, point, scheme_cell.fg, end_offset);
                }

                // Push the new cell into URL
                self.extend_url(point, end, cell.fg, end_offset);
            },
            (UrlLocation::Url(_length, end_offset), UrlLocation::Url(..)) => {
                self.extend_url(point, end, cell.fg, end_offset);
            },
            (UrlLocation::Scheme, _) => self.scheme_buffer.push(cell),
            (UrlLocation::Reset, _) => self.reset(),
            _ => (),
        }

        // Reset at un-wrapped linebreak
        if cell.column.0 + 1 == num_cols && !cell.flags.contains(Flags::WRAPLINE) {
            self.reset();
        }
    }

    // Extend the last URL
    fn extend_url(&mut self, start: Point, end: Point, color: Rgb, end_offset: u16) {
        let url = self.urls.last_mut().unwrap();

        // If color changed, we need to insert a new line
        if url.lines.last().map(|last| last.color) == Some(color) {
            url.lines.last_mut().unwrap().end = end;
        } else {
            url.lines.push(RenderLine { color, start, end });
        }

        // Update excluded cells at the end of the URL
        url.end_offset = end_offset;
    }

    pub fn highlighted(
        &self,
        config: &Config,
        mouse: &Mouse,
        mods: ModifiersState,
        mouse_mode: bool,
        selection: bool,
    ) -> Option<Url> {
        // Make sure all prerequisites for highlighting are met
        if selection
            || (mouse_mode && !mods.shift)
            || !mouse.inside_grid
            || config.ui_config.mouse.url.launcher.is_none()
            || !config.ui_config.mouse.url.mods().relaxed_eq(mods)
            || mouse.left_button_state == ElementState::Pressed
        {
            return None;
        }

        for url in &self.urls {
            if (url.start()..=url.end()).contains(&Point::new(mouse.line, mouse.column)) {
                return Some(url.clone());
            }
        }

        None
    }

    fn reset(&mut self) {
        self.locator = UrlLocator::new();
        self.state = UrlLocation::Reset;
        self.scheme_buffer.clear();
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::cell::MAX_ZEROWIDTH_CHARS;

    fn text_to_cells(text: &str) -> Vec<RenderableCell> {
        text.chars()
            .enumerate()
            .map(|(i, c)| RenderableCell {
                inner: RenderableCellContent::Chars([c; MAX_ZEROWIDTH_CHARS + 1]),
                line: Line(0),
                column: Column(i),
                fg: Default::default(),
                bg: Default::default(),
                bg_alpha: 0.,
                flags: Flags::empty(),
            })
            .collect()
    }

    #[test]
    fn multi_color_url() {
        let mut input = text_to_cells("test https://example.org ing");
        let num_cols = input.len();

        input[10].fg = Rgb { r: 0xff, g: 0x00, b: 0xff };

        let mut urls = Urls::new();

        for cell in input {
            urls.update(num_cols, cell);
        }

        let url = urls.urls.first().unwrap();
        assert_eq!(url.start().col, Column(5));
        assert_eq!(url.end().col, Column(23));
    }

    #[test]
    fn multiple_urls() {
        let input = text_to_cells("test git:a git:b git:c ing");
        let num_cols = input.len();

        let mut urls = Urls::new();

        for cell in input {
            urls.update(num_cols, cell);
        }

        assert_eq!(urls.urls.len(), 3);

        assert_eq!(urls.urls[0].start().col, Column(5));
        assert_eq!(urls.urls[0].end().col, Column(9));

        assert_eq!(urls.urls[1].start().col, Column(11));
        assert_eq!(urls.urls[1].end().col, Column(15));

        assert_eq!(urls.urls[2].start().col, Column(17));
        assert_eq!(urls.urls[2].end().col, Column(21));
    }
}
