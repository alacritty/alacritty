use std::iter::Peekable;
use std::{cmp, mem};

use glutin::surface::Rect;

use alacritty_terminal::index::Point;
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::{LineDamageBounds, TermDamageIterator};

use crate::display::SizeInfo;

/// State of the damage tracking for the [`Display`].
///
/// [`Display`]: crate::display::Display
#[derive(Debug)]
pub struct DamageTracker {
    /// Position of the previously drawn Vi cursor.
    pub old_vi_cursor: Option<Point<usize>>,
    /// The location of the old selection.
    pub old_selection: Option<SelectionRange>,
    /// Highlight damage submitted for the compositor.
    pub debug: bool,

    /// The damage for the frames.
    frames: [FrameDamage; 2],
    screen_lines: usize,
    columns: usize,
}

impl DamageTracker {
    pub fn new(screen_lines: usize, columns: usize) -> Self {
        let mut tracker = Self {
            columns,
            screen_lines,
            debug: false,
            old_vi_cursor: None,
            old_selection: None,
            frames: Default::default(),
        };
        tracker.resize(screen_lines, columns);
        tracker
    }

    #[inline]
    #[must_use]
    pub fn frame(&mut self) -> &mut FrameDamage {
        &mut self.frames[0]
    }

    #[inline]
    #[must_use]
    pub fn next_frame(&mut self) -> &mut FrameDamage {
        &mut self.frames[1]
    }

    /// Advance to the next frame resetting the state for the active frame.
    #[inline]
    pub fn swap_damage(&mut self) {
        let screen_lines = self.screen_lines;
        let columns = self.columns;
        self.frame().reset(screen_lines, columns);
        self.frames.swap(0, 1);
    }

    /// Resize the damage information in the tracker.
    pub fn resize(&mut self, screen_lines: usize, columns: usize) {
        self.screen_lines = screen_lines;
        self.columns = columns;
        for frame in &mut self.frames {
            frame.reset(screen_lines, columns);
        }
        self.frame().full = true;
    }

    /// Damage vi cursor inside the viewport.
    pub fn damage_vi_cursor(&mut self, mut vi_cursor: Option<Point<usize>>) {
        mem::swap(&mut self.old_vi_cursor, &mut vi_cursor);

        if self.frame().full {
            return;
        }

        if let Some(vi_cursor) = self.old_vi_cursor {
            self.frame().damage_point(vi_cursor);
        }

        if let Some(vi_cursor) = vi_cursor {
            self.frame().damage_point(vi_cursor);
        }
    }

    /// Get shaped frame damage for the active frame.
    pub fn shape_frame_damage(&self, size_info: SizeInfo<u32>) -> Vec<Rect> {
        if self.frames[0].full {
            vec![Rect::new(0, 0, size_info.width() as i32, size_info.height() as i32)]
        } else {
            let lines_damage = RenderDamageIterator::new(
                TermDamageIterator::new(&self.frames[0].lines, 0),
                &size_info,
            );
            lines_damage.chain(self.frames[0].rects.iter().copied()).collect()
        }
    }

    /// Add the current frame's selection damage.
    pub fn damage_selection(
        &mut self,
        mut selection: Option<SelectionRange>,
        display_offset: usize,
    ) {
        mem::swap(&mut self.old_selection, &mut selection);

        if self.frame().full || selection == self.old_selection {
            return;
        }

        for selection in self.old_selection.into_iter().chain(selection) {
            let display_offset = display_offset as i32;
            let last_visible_line = self.screen_lines as i32 - 1;
            let columns = self.columns;

            // Ignore invisible selection.
            if selection.end.line.0 + display_offset < 0
                || selection.start.line.0.abs() < display_offset - last_visible_line
            {
                continue;
            };

            let start = cmp::max(selection.start.line.0 + display_offset, 0) as usize;
            let end = (selection.end.line.0 + display_offset).clamp(0, last_visible_line) as usize;
            for line in start..=end {
                self.frame().lines[line].expand(0, columns - 1);
            }
        }
    }
}

/// Damage state for the rendering frame.
#[derive(Debug, Default)]
pub struct FrameDamage {
    /// The entire frame needs to be redrawn.
    full: bool,
    /// Terminal lines damaged in the given frame.
    lines: Vec<LineDamageBounds>,
    /// Rectangular regions damage in the given frame.
    rects: Vec<Rect>,
}

impl FrameDamage {
    /// Damage line for the given frame.
    #[inline]
    pub fn damage_line(&mut self, damage: LineDamageBounds) {
        self.lines[damage.line].expand(damage.left, damage.right);
    }

    #[inline]
    pub fn damage_point(&mut self, point: Point<usize>) {
        self.lines[point.line].expand(point.column.0, point.column.0);
    }

    /// Mark the frame as fully damaged.
    #[inline]
    pub fn mark_fully_damaged(&mut self) {
        self.full = true;
    }

    /// Add viewport rectangle to damage.
    ///
    /// This allows covering elements outside of the terminal viewport, like message bar.
    #[inline]
    pub fn add_viewport_rect(
        &mut self,
        size_info: &SizeInfo,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) {
        let y = viewport_y_to_damage_y(size_info, y, height);
        self.rects.push(Rect { x, y, width, height });
    }

    fn reset(&mut self, num_lines: usize, num_cols: usize) {
        self.full = false;
        self.rects.clear();
        self.lines.clear();
        self.lines.reserve(num_lines);
        for line in 0..num_lines {
            self.lines.push(LineDamageBounds::undamaged(line, num_cols));
        }
    }

    /// Check if a range is damaged.
    #[inline]
    pub fn intersects(&self, start: Point<usize>, end: Point<usize>) -> bool {
        let start_line = &self.lines[start.line];
        let end_line = &self.lines[end.line];
        self.full
            || (start_line.left..=start_line.right).contains(&start.column)
            || (end_line.left..=end_line.right).contains(&end.column)
            || (start.line + 1..end.line).any(|line| self.lines[line].is_damaged())
    }
}

/// Convert viewport `y` coordinate to [`Rect`] damage coordinate.
pub fn viewport_y_to_damage_y(size_info: &SizeInfo, y: i32, height: i32) -> i32 {
    size_info.height() as i32 - y - height
}

/// Convert viewport `y` coordinate to [`Rect`] damage coordinate.
pub fn damage_y_to_viewport_y(size_info: &SizeInfo, rect: &Rect) -> i32 {
    size_info.height() as i32 - rect.y - rect.height
}

/// Iterator which converts `alacritty_terminal` damage information into renderer damaged rects.
struct RenderDamageIterator<'a> {
    damaged_lines: Peekable<TermDamageIterator<'a>>,
    size_info: &'a SizeInfo<u32>,
}

impl<'a> RenderDamageIterator<'a> {
    pub fn new(damaged_lines: TermDamageIterator<'a>, size_info: &'a SizeInfo<u32>) -> Self {
        Self { damaged_lines: damaged_lines.peekable(), size_info }
    }

    #[inline]
    fn rect_for_line(&self, line_damage: LineDamageBounds) -> Rect {
        let size_info = &self.size_info;
        let y_top = size_info.height() - size_info.padding_y();
        let x = size_info.padding_x() + line_damage.left as u32 * size_info.cell_width();
        let y = y_top - (line_damage.line + 1) as u32 * size_info.cell_height();
        let width = (line_damage.right - line_damage.left + 1) as u32 * size_info.cell_width();
        Rect::new(x as i32, y as i32, width as i32, size_info.cell_height() as i32)
    }

    // Make sure to damage near cells to include wide chars.
    #[inline]
    fn overdamage(size_info: &SizeInfo<u32>, mut rect: Rect) -> Rect {
        rect.x = (rect.x - size_info.cell_width() as i32).max(0);
        rect.width = cmp::min(
            (size_info.width() as i32 - rect.x).max(0),
            rect.width + 2 * size_info.cell_width() as i32,
        );
        rect.y = (rect.y - size_info.cell_height() as i32 / 2).max(0);
        rect.height = cmp::min(
            (size_info.height() as i32 - rect.y).max(0),
            rect.height + size_info.cell_height() as i32,
        );

        rect
    }
}

impl Iterator for RenderDamageIterator<'_> {
    type Item = Rect;

    fn next(&mut self) -> Option<Rect> {
        let line = self.damaged_lines.next()?;
        let size_info = &self.size_info;
        let mut total_damage_rect = Self::overdamage(size_info, self.rect_for_line(line));

        // Merge rectangles which overlap with each other.
        while let Some(line) = self.damaged_lines.peek().copied() {
            let next_rect = Self::overdamage(size_info, self.rect_for_line(line));
            if !rects_overlap(total_damage_rect, next_rect) {
                break;
            }

            total_damage_rect = merge_rects(total_damage_rect, next_rect);
            let _ = self.damaged_lines.next();
        }

        Some(total_damage_rect)
    }
}

/// Check if two given [`glutin::surface::Rect`] overlap.
fn rects_overlap(lhs: Rect, rhs: Rect) -> bool {
    !(
        // `lhs` is left of `rhs`.
        lhs.x + lhs.width < rhs.x
        // `lhs` is right of `rhs`.
        || rhs.x + rhs.width < lhs.x
        // `lhs` is below `rhs`.
        || lhs.y + lhs.height < rhs.y
        // `lhs` is above `rhs`.
        || rhs.y + rhs.height < lhs.y
    )
}

/// Merge two [`glutin::surface::Rect`] by producing the smallest rectangle that contains both.
#[inline]
fn merge_rects(lhs: Rect, rhs: Rect) -> Rect {
    let left_x = cmp::min(lhs.x, rhs.x);
    let right_x = cmp::max(lhs.x + lhs.width, rhs.x + rhs.width);
    let y_top = cmp::max(lhs.y + lhs.height, rhs.y + rhs.height);
    let y_bottom = cmp::min(lhs.y, rhs.y);
    Rect::new(left_x, y_bottom, right_x - left_x, y_top - y_bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_rect_math() {
        let rect_side = 10;
        let cell_size = 4;
        let bound = 100;

        let size_info: SizeInfo<u32> = SizeInfo::new(
            bound as f32,
            bound as f32,
            cell_size as f32,
            cell_size as f32,
            2.,
            2.,
            true,
        )
        .into();

        // Test min clamping.
        let rect = Rect::new(0, 0, rect_side, rect_side);
        let rect = RenderDamageIterator::overdamage(&size_info, rect);
        assert_eq!(Rect::new(0, 0, rect_side + 2 * cell_size, 10 + cell_size), rect);

        // Test max clamping.
        let rect = Rect::new(bound, bound, rect_side, rect_side);
        let rect = RenderDamageIterator::overdamage(&size_info, rect);
        assert_eq!(
            Rect::new(bound - cell_size, bound - cell_size / 2, cell_size, cell_size / 2),
            rect
        );

        // Test no clamping.
        let rect = Rect::new(bound / 2, bound / 2, rect_side, rect_side);
        let rect = RenderDamageIterator::overdamage(&size_info, rect);
        assert_eq!(
            Rect::new(
                bound / 2 - cell_size,
                bound / 2 - cell_size / 2,
                rect_side + 2 * cell_size,
                rect_side + cell_size
            ),
            rect
        );

        // Test out of bounds coord clamping.
        let rect = Rect::new(bound * 2, bound * 2, rect_side, rect_side);
        let rect = RenderDamageIterator::overdamage(&size_info, rect);
        assert_eq!(Rect::new(bound * 2 - cell_size, bound * 2 - cell_size / 2, 0, 0), rect);
    }

    #[test]
    fn add_viewport_damage() {
        let mut frame_damage = FrameDamage::default();
        let viewport_height = 100.;
        let x = 0;
        let y = 40;
        let height = 5;
        let width = 10;
        let size_info = SizeInfo::new(viewport_height, viewport_height, 5., 5., 0., 0., true);
        frame_damage.add_viewport_rect(&size_info, x, y, width, height);
        assert_eq!(frame_damage.rects[0], Rect {
            x,
            y: viewport_height as i32 - y - height,
            width,
            height
        });
        assert_eq!(frame_damage.rects[0].y, viewport_y_to_damage_y(&size_info, y, height));
        assert_eq!(damage_y_to_viewport_y(&size_info, &frame_damage.rects[0]), y);
    }
}
