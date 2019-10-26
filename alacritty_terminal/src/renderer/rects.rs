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

use crate::term::color::Rgb;
use crate::term::SizeInfo;
use crate::text_run::TextRun;

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

    pub fn from_text_run(
        text_run: &TextRun,
        (descent, position, thickness): (f32, f32, f32),
        size: &SizeInfo,
    ) -> Self {
        let start_point = text_run.start_point();
        let start_x = start_point.col.0 as f32 * size.cell_width;
        let end_x = (text_run.end_point().col.0 + 1) as f32 * size.cell_width;
        let width = end_x - start_x;

        let line_bottom = (start_point.line.0 as f32 + 1.) * size.cell_height;
        let baseline = line_bottom + descent;

        // Make sure lines are always visible
        let height = thickness.max(1.);

        let mut y = baseline - position - height / 2.;
        let max_y = line_bottom - height;
        if y > max_y {
            y = max_y;
        }

        RenderRect::new(start_x + size.padding_x, y + size.padding_y, width, height, self.color, 1.)
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

    pub fn into_rects(self, metrics: &Metrics, size: &SizeInfo) -> Vec<RenderRect> {
        self.inner
            .into_iter()
            .map(|(flag, lines)| -> Vec<RenderRect> {
                lines.into_iter().map(|line| line.into_rect(flag, &metrics, &size)).collect()
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
                if cell.line == line.start.line
                    && cell.fg == line.color
                    && cell.column == line.end.col + 1
                {
                    // Update the length of the line
                    line.end = cell.into();
                    continue;
                }
            }

        RenderRect::new(start_x + size.padding_x, y + size.padding_y, width, height, text_run.fg)
    }
}
