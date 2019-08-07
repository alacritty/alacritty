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
use std::collections::HashMap;

use font::Metrics;

use crate::index::Point;
use crate::term::cell::Flags;
use crate::term::color::Rgb;
#[cfg(feature = "hb-ft")]
use crate::term::text_run::TextRun;
#[cfg(not(feature = "hb-ft"))]
use crate::term::RenderableCell;
use crate::term::SizeInfo;

#[derive(Debug, Copy, Clone)]
pub struct Rect<T> {
    pub x: T,
    pub y: T,
    pub width: T,
    pub height: T,
}

impl<T> Rect<T> {
    pub fn new(x: T, y: T, width: T, height: T) -> Self {
        Rect { x, y, width, height }
    }
}

struct Line {
    rect: Rect<f32>,
    start: Point,
    color: Rgb,
}

impl Line {
    #[cfg(not(feature = "hb-ft"))]
    /// Create a line that starts on the left of `cell` and is one cell wide
    fn from_cell(cell: &RenderableCell, flag: Flags, metrics: &Metrics, size: &SizeInfo) -> Line {
        let cell_x = cell.column.0 as f32 * size.cell_width;

        let (position, mut height) = match flag {
            Flags::UNDERLINE => (metrics.underline_position, metrics.underline_thickness),
            Flags::STRIKEOUT => (metrics.strikeout_position, metrics.strikeout_thickness),
            _ => unimplemented!("Invalid flag for cell line drawing specified"),
        };

        // Make sure lines are always visible
        height = height.max(1.);

        let cell_bottom = (cell.line.0 as f32 + 1.) * size.cell_height;
        let baseline = cell_bottom + metrics.descent;

        let mut y = baseline - position - height / 2.;
        let max_y = cell_bottom - height;
        if y > max_y {
            y = max_y;
        }

        let rect = Rect::new(cell_x + size.padding_x, y + size.padding_y, size.cell_width, height);

        Self { start: cell.into(), color: cell.fg, rect }
    }

    #[cfg(not(feature = "hb-ft"))]
    fn update_end(&mut self, end: Point, size: &SizeInfo) {
        self.rect.width = (end.col + 1 - self.start.col).0 as f32 * size.cell_width;
    }

    #[cfg(feature = "hb-ft")]
    /// Create a line that starts on the left of `text_run` and is `text_run.len()` wide
    fn from_text_run(text_run: &TextRun, flag: Flags, metrics: &Metrics, size: &SizeInfo) -> Self {
        // This is basically a 1:1 copy of from_cell but with semantics of TextRun
        let run_x = text_run.start_col().0 as f32 * size.cell_width;

        let (position, mut height) = match flag {
            Flags::UNDERLINE => (metrics.underline_position, metrics.underline_thickness),
            Flags::STRIKEOUT => (metrics.strikeout_position, metrics.strikeout_thickness),
            _ => unimplemented!("Invalid flag for text run line drawing specified"),
        };

        // Make sure lines are always visible
        height = height.max(1.);

        let run_bottom = (text_run.line.0 as f32 + 1.) * size.cell_height;
        let baseline = run_bottom + metrics.descent;

        let mut y = baseline - position - height / 2.;
        let max_y = run_bottom - height;
        if y > max_y {
            y = max_y;
        }

        let rect = Rect::new(run_x + size.padding_x, y + size.padding_y, (text_run.len() + 1) as f32 * size.cell_width, height);

        Self { start: text_run.start_point(), color: text_run.fg, rect }
    }
}

/// Rects for underline, strikeout and more.
#[derive(Default)]
pub struct Rects {
    inner: HashMap<Flags, Vec<Line>>,
}

impl Rects {
    pub fn new() -> Self {
        Self::default()
    }

    /// Convert the stored rects to rectangles for the renderer.
    pub fn rects(&self) -> Vec<(Rect<f32>, Rgb)> {
        self.inner
            .iter()
            .map(|(_, lines)| lines)
            .flatten()
            .map(|line| (line.rect, line.color))
            .collect()
    }

    #[cfg(feature = "hb-ft")]
    /// Update the stored lines with the next text_run info.
    pub fn update_lines_text_run(
        &mut self,
        text_run: &TextRun,
        size: &SizeInfo,
        metrics: &Metrics,
    ) {
        for flag in &[Flags::UNDERLINE, Flags::STRIKEOUT] {
            if !text_run.flags.contains(*flag) {
                continue;
            }

            let new_line = Line::from_text_run(text_run, *flag, metrics, size);
            self.inner.entry(*flag).or_insert_with(|| vec![]).push(new_line);
        }
    }

    /// Update the stored lines with the next cell info.
    #[cfg(not(feature = "hb-ft"))]
    pub fn update_lines(&mut self, cell: &RenderableCell, size: &SizeInfo, metrics: &Metrics) {
        for flag in &[Flags::UNDERLINE, Flags::STRIKEOUT] {
            if !cell.flags.contains(*flag) {
                continue;
            }

            // Check if there's an active line
            if let Some(line) = self.inner.get_mut(flag).and_then(|lines| lines.last_mut()) {
                if cell.line == line.start.line && cell.fg == line.color {
                    // Update the length of the line
                    line.update_end(cell.into(), size);

                    continue;
                }
            }

            // Start new line if there currently is none
            let rect = Line::from_cell(cell, *flag, metrics, size);
            match self.inner.get_mut(flag) {
                Some(lines) => lines.push(rect),
                None => {
                    self.inner.insert(*flag, vec![rect]);
                },
            }
        }
    }

    // Add a rectangle
    pub fn push(&mut self, rect: Rect<f32>, color: Rgb) {
        let line = Line { start: Point::default(), color, rect };

        // Flag `HIDDEN` for hashmap index is arbitrary
        match self.inner.get_mut(&Flags::HIDDEN) {
            Some(lines) => lines.push(line),
            None => {
                self.inner.insert(Flags::HIDDEN, vec![line]);
            },
        }
    }
}
