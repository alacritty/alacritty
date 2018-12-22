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

use crate::term::{ SizeInfo, RenderableCell};
use crate::term::cell::Flags;
use crate::renderer::Rect;
use font::Metrics;
use crate::Rgb;

/// Lines for underline and strikeout.
pub struct Lines<'a> {
    inner: Vec<(Rect<f32>, Rgb)>,
    last_starts: HashMap<Flags, Option<RenderableCell>>,
    last_cell: Option<RenderableCell>,
    metrics: &'a Metrics,
    size: &'a SizeInfo,
}

impl<'a> Lines<'a> {
    pub fn new(metrics: &'a Metrics, size: &'a SizeInfo) -> Self {
        let mut last_starts = HashMap::new();
        last_starts.insert(Flags::UNDERLINE, None);
        last_starts.insert(Flags::STRIKEOUT, None);

        Self {
            inner: Vec::new(),
            last_cell: None,
            last_starts,
            metrics,
            size,
        }
    }

    /// Convert the stored lines to rectangles for the renderer.
    pub fn rects(mut self) -> Vec<(Rect<f32>, Rgb)> {
        // If there's still a line pending, draw it until the last cell
        for (flag, start_cell) in self.last_starts.iter_mut() {
            if let Some(start) = start_cell {
                self.inner.push(
                    create_rect(
                        &start,
                        &self.last_cell.unwrap(),
                        *flag,
                        &self.metrics,
                        &self.size,
                    )
                );
            }
        }

        self.inner
    }

    /// Update the stored lines with the next cell info.
    pub fn update_lines(&mut self, cell: &RenderableCell) {
        for (flag, start_cell) in self.last_starts.iter_mut() {
            let flag = *flag;
            *start_cell = match *start_cell {
                // Check for end if line is present
                Some(ref mut start) => {
                    // No change in line
                    if cell.line == start.line && cell.flags.contains(flag) && cell.fg == start.fg {
                        continue;
                    }

                    self.inner.push(
                        create_rect(
                            &start,
                            &self.last_cell.unwrap(),
                            flag,
                            &self.metrics,
                            &self.size,
                        )
                    );

                    // Start a new line if the flag is present
                    if cell.flags.contains(flag) {
                        Some(*cell)
                    } else {
                        None
                    }
                }
                // Check for new start of line
                None => if cell.flags.contains(flag) {
                    Some(*cell)
                } else {
                    None
                },
            };
        }

        self.last_cell = Some(*cell);
    }
}

/// Create a rectangle between two cells.
fn create_rect(
    start: &RenderableCell,
    end: &RenderableCell,
    flag: Flags,
    metrics: &Metrics,
    size: &SizeInfo,
) -> (Rect<f32>, Rgb) {
    let start_x = start.column.0 as f32 * size.cell_width;
    let end_x = (end.column.0 + 1) as f32 * size.cell_width;
    let width = end_x - start_x;

    let (position, height) = match flag {
        Flags::UNDERLINE => (metrics.underline_position, metrics.underline_thickness),
        Flags::STRIKEOUT => (metrics.strikeout_position, metrics.strikeout_thickness),
        _ => unimplemented!("Invalid flag for cell line drawing specified"),
    };

    let cell_bottom = (start.line.0 as f32 + 1.) * size.cell_height;
    let baseline = cell_bottom + metrics.descent;

    let mut y = baseline - position - height / 2.;
    let max_y = cell_bottom - height;
    if y > max_y {
        y = max_y;
    }

    let rect = Rect::new(
        start_x + size.padding_x,
        y + size.padding_y,
        width,
        height,
    );

    (rect, start.fg)
}
