use std::collections::HashMap;

use crossfont::Metrics;

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
            let end = Point::new(start.line, size.cols() - 1);
            Self::push_rects(&mut rects, metrics, size, flag, start, end, self.color);
            start = Point::new(start.line + 1, Column(0));
        }
        Self::push_rects(&mut rects, metrics, size, flag, start, self.end, self.color);

        rects
    }

    /// Push all rects required to draw the cell's line.
    fn push_rects(
        rects: &mut Vec<RenderRect>,
        metrics: &Metrics,
        size: &SizeInfo,
        flag: Flags,
        start: Point,
        end: Point,
        color: Rgb,
    ) {
        let (position, thickness) = match flag {
            Flags::DOUBLE_UNDERLINE => {
                // Position underlines so each one has 50% of descent available.
                let top_pos = 0.25 * metrics.descent;
                let bottom_pos = 0.75 * metrics.descent;

                rects.push(Self::create_rect(
                    size,
                    metrics.descent,
                    start,
                    end,
                    top_pos,
                    metrics.underline_thickness,
                    color,
                ));

                (bottom_pos, metrics.underline_thickness)
            },
            Flags::UNDERLINE => (metrics.underline_position, metrics.underline_thickness),
            Flags::STRIKEOUT => (metrics.strikeout_position, metrics.strikeout_thickness),
            _ => unimplemented!("Invalid flag for cell line drawing specified"),
        };

        rects.push(Self::create_rect(
            size,
            metrics.descent,
            start,
            end,
            position,
            thickness,
            color,
        ));
    }

    /// Create a line's rect at a position relative to the baseline.
    fn create_rect(
        size: &SizeInfo,
        descent: f32,
        start: Point,
        end: Point,
        position: f32,
        mut thickness: f32,
        color: Rgb,
    ) -> RenderRect {
        let start_x = start.col.0 as f32 * size.cell_width;
        let end_x = (end.col.0 + 1) as f32 * size.cell_width;
        let width = end_x - start_x;

        // Make sure lines are always visible.
        thickness = thickness.max(1.);

        let line_bottom = (start.line.0 as f32 + 1.) * size.cell_height;
        let baseline = line_bottom + descent;

        let mut y = (baseline - position - thickness / 2.).ceil();
        let max_y = line_bottom - thickness;
        if y > max_y {
            y = max_y;
        }

        RenderRect::new(start_x + size.padding_x, y + size.padding_y, width, thickness, color, 1.)
    }
}

/// Lines for underline and strikeout.
#[derive(Default)]
pub struct RenderLines {
    inner: HashMap<Flags, Vec<RenderLine>>,
}

impl RenderLines {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
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
    #[inline]
    pub fn update(&mut self, cell: RenderableCell) {
        self.update_flag(cell, Flags::UNDERLINE);
        self.update_flag(cell, Flags::DOUBLE_UNDERLINE);
        self.update_flag(cell, Flags::STRIKEOUT);
    }

    /// Update the lines for a specific flag.
    fn update_flag(&mut self, cell: RenderableCell, flag: Flags) {
        if !cell.flags.contains(flag) {
            return;
        }

        // Check if there's an active line.
        if let Some(line) = self.inner.get_mut(&flag).and_then(|lines| lines.last_mut()) {
            if cell.fg == line.color
                && cell.column == line.end.col + 1
                && cell.line == line.end.line
            {
                // Update the length of the line.
                line.end = cell.into();
                return;
            }
        }

        // Start new line if there currently is none.
        let line = RenderLine { start: cell.into(), end: cell.into(), color: cell.fg };
        match self.inner.get_mut(&flag) {
            Some(lines) => lines.push(line),
            None => {
                self.inner.insert(flag, vec![line]);
            },
        }
    }
}
