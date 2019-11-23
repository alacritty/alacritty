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

use alacritty_terminal::index::{Column, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Rgb;
use alacritty_terminal::term::{RenderableCell, SizeInfo};

#[derive(Debug, Copy, Clone)]
pub struct RenderRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: Rgb,
    pub alpha: f32,
}

impl RenderRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32, color: Rgb, alpha: f32) -> Self {
        RenderRect { x, y, width, height, color, alpha }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct RenderLine {
    pub start: Point,
    pub end: Point,
    pub color: Rgb,
}

impl RenderLine {
    pub fn rects(&self, flag: Flags, metrics: &Metrics, size: &SizeInfo) -> Vec<RenderRect> {
        let mut rects = Vec::new();

        let mut start = self.start;
        while start.line < self.end.line {
            let mut end = start;
            end.col = size.cols() - 1;
            rects.push(Self::create_rect(metrics, size, flag, start, end, self.color));

            start.col = Column(0);
            start.line += 1;
        }

        rects.push(Self::create_rect(metrics, size, flag, start, self.end, self.color));

        rects
    }

    fn create_rect(
        metrics: &Metrics,
        size: &SizeInfo,
        flag: Flags,
        start: Point,
        end: Point,
        color: Rgb,
    ) -> RenderRect {
        let start_x = start.col.0 as f32 * size.cell_width;
        let end_x = (end.col.0 + 1) as f32 * size.cell_width;
        let width = end_x - start_x;

        let (position, mut height) = match flag {
            Flags::UNDERLINE => (metrics.underline_position, metrics.underline_thickness),
            Flags::STRIKEOUT => (metrics.strikeout_position, metrics.strikeout_thickness),
            _ => unimplemented!("Invalid flag for cell line drawing specified"),
        };

        // Make sure lines are always visible
        height = height.max(1.);

        let line_bottom = (start.line.0 as f32 + 1.) * size.cell_height;
        let baseline = line_bottom + metrics.descent;

        let mut y = baseline - position - height / 2.;
        let max_y = line_bottom - height;
        if y > max_y {
            y = max_y;
        }

        RenderRect::new(start_x + size.padding_x, y + size.padding_y, width, height, color, 1.)
    }
}

/// Lines for underline and strikeout.
#[derive(Default)]
pub struct RenderLines {
    inner: HashMap<Flags, Vec<RenderLine>>,
}

impl RenderLines {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn rects(&self, metrics: &Metrics, size: &SizeInfo) -> Vec<RenderRect> {
        self.inner
            .iter()
            .map(|(flag, lines)| -> Vec<RenderRect> {
                lines.iter().map(|line| line.rects(*flag, metrics, size)).flatten().collect()
            })
            .flatten()
            .collect()
    }

    /// Update the stored lines with the next cell info.
    pub fn update(&mut self, cell: RenderableCell) {
        for flag in &[Flags::UNDERLINE, Flags::STRIKEOUT] {
            if !cell.flags.contains(*flag) {
                continue;
            }

            // Check if there's an active line
            if let Some(line) = self.inner.get_mut(flag).and_then(|lines| lines.last_mut()) {
                if cell.fg == line.color
                    && cell.column == line.end.col + 1
                    && cell.line == line.end.line
                {
                    // Update the length of the line
                    line.end = cell.into();
                    continue;
                }
            }

            // Start new line if there currently is none
            let line = RenderLine { start: cell.into(), end: cell.into(), color: cell.fg };
            match self.inner.get_mut(flag) {
                Some(lines) => lines.push(line),
                None => {
                    self.inner.insert(*flag, vec![line]);
                },
            }
        }
    }
}
