use std::cmp::min;
use std::mem;

use glutin::event::{ElementState, ModifiersState};
use urlocator::{UrlLocation, UrlLocator};

use font::Metrics;

use alacritty_terminal::index::Point;
use alacritty_terminal::renderer::rects::{RenderLine, RenderRect};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{RenderableCell, RenderableCellContent, SizeInfo};

use crate::config::{Config, RelaxedEq};
use crate::event::Mouse;

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
    last_point: Option<Point>,
    state: UrlLocation,
}

impl Default for Urls {
    fn default() -> Self {
        Self {
            locator: UrlLocator::new(),
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

        // Reset URL when empty cells have been skipped
        if point != Point::default() && Some(point.sub(num_cols, 1)) != self.last_point {
            self.reset();
        }
        self.last_point = Some(point);

        // Advance parser
        let last_state = mem::replace(&mut self.state, self.locator.advance(c));
        match (self.state, last_state) {
            (UrlLocation::Url(_length, end_offset), _) => {
                let mut end = point;

                // Extend by one cell for double-width characters
                if cell.flags.contains(Flags::WIDE_CHAR) {
                    end.col += 1;

                    self.last_point = Some(end);
                }

                if let Some(url) = self.urls.last_mut() {
                    let last_index = url.lines.len() - 1;
                    let last_line = &mut url.lines[last_index];

                    if last_line.color == cell.fg {
                        // Update existing line
                        last_line.end = end;
                    } else {
                        // Create new line with different color
                        url.lines.push(RenderLine { start: point, end, color: cell.fg });
                    }

                    // Update offset
                    url.end_offset = end_offset;
                }
            },
            (UrlLocation::Reset, UrlLocation::Scheme) => {
                self.urls.pop();
            },
            (UrlLocation::Scheme, UrlLocation::Reset) => {
                self.urls.push(Url {
                    lines: vec![RenderLine { start: point, end: point, color: cell.fg }],
                    end_offset: 0,
                    num_cols,
                });
            },
            (UrlLocation::Scheme, _) => {
                if let Some(url) = self.urls.last_mut() {
                    if let Some(last_line) = url.lines.last_mut() {
                        if last_line.color == cell.fg {
                            last_line.end = point;
                        } else {
                            url.lines.push(RenderLine { start: point, end: point, color: cell.fg });
                        }
                    }
                }
            },
            _ => (),
        }

        // Reset at un-wrapped linebreak
        if cell.column.0 + 1 == num_cols && !cell.flags.contains(Flags::WRAPLINE) {
            self.reset();
        }
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
        // Remove temporarily stored scheme URLs
        if let UrlLocation::Scheme = self.state {
            self.urls.pop();
        }

        self.locator = UrlLocator::new();
        self.state = UrlLocation::Reset;
    }
}
