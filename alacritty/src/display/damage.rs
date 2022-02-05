use std::cmp;
use std::iter::Peekable;

use glutin::Rect;

use alacritty_terminal::term::{LineDamageBounds, SizeInfo, TermDamageIterator};

/// Maximum percent of area growth from merging damaged rects.
const MAX_GROWTH: f32 = 0.3;

/// Iterator which converts `alacritty_terminal` damage information into renderer damaged rects.
pub struct RenderDamageIterator<'a> {
    damaged_lines: Peekable<TermDamageIterator<'a>>,
    size_info: SizeInfo<u32>,
}

impl<'a> RenderDamageIterator<'a> {
    pub fn new(damaged_lines: TermDamageIterator<'a>, size_info: SizeInfo<u32>) -> Self {
        Self { damaged_lines: damaged_lines.peekable(), size_info }
    }

    #[inline]
    fn rect_for_line(&self, line_damage: LineDamageBounds) -> Rect {
        let size_info = &self.size_info;
        let y_top = size_info.height() - size_info.padding_y();
        let x = size_info.padding_x() + line_damage.left as u32 * size_info.cell_width();
        let y = y_top - (line_damage.line + 1) as u32 * size_info.cell_height();
        let width = (line_damage.right - line_damage.left + 1) as u32 * size_info.cell_width();
        Rect { x, y, height: size_info.cell_height(), width }
    }

    // Make sure to damage near cells to include wide chars.
    #[inline]
    fn overdamage(&self, mut rect: Rect) -> Rect {
        let size_info = &self.size_info;
        rect.x = rect.x.saturating_sub(size_info.cell_width());
        rect.width = cmp::min(size_info.width() - rect.x, rect.width + 2 * size_info.cell_width());
        rect.y = rect.y.saturating_sub(size_info.cell_height() / 2);
        rect.height = cmp::min(size_info.height() - rect.y, rect.height + size_info.cell_height());

        rect
    }
}

impl<'a> Iterator for RenderDamageIterator<'a> {
    type Item = Rect;

    fn next(&mut self) -> Option<Rect> {
        let line = self.damaged_lines.next()?;
        let mut total_damage_rect = self.overdamage(self.rect_for_line(line));

        // We don't want to merge `total_damage_rect` with lines that a much longer/shorter than
        // it, since we'd end up damaging in suboptimal way.
        let max_width_growth = self.size_info.width() / 3;

        // Merge rectangles which overlap with each other, unless they don't grow by
        // `max_width_growth` at once.
        while let Some(line) = self.damaged_lines.peek().copied() {
            let next_rect = self.overdamage(self.rect_for_line(line));
            if !rects_overlap(&total_damage_rect, &next_rect) {
                break;
            }

            let merged_rect = merge_rects(total_damage_rect, next_rect);

            // If the width growth is higher than `max_width_growth` don't merge rects and the
            // area growth is larger than `MAX_GROWTH` to avoid overdamaging.
            if width_growth(&total_damage_rect, &next_rect) > max_width_growth
                && area_growth(&total_damage_rect, &merged_rect) > MAX_GROWTH
            {
                break;
            }

            total_damage_rect = merged_rect;
            let _ = self.damaged_lines.next();
        }

        Some(total_damage_rect)
    }
}

/// The growth of width of intersected `[glutin::Rect]`.
fn width_growth(lhs: &Rect, rhs: &Rect) -> u32 {
    let lhs_x_end = (lhs.x + lhs.width) as i32;
    let rhs_x_end = (rhs.x + rhs.width) as i32;

    // The amount damage rect width growth.
    (lhs.x as i32 - rhs.x as i32).abs() as u32 + (lhs_x_end - rhs_x_end).abs() as u32
}

/// Area occupied by `[glutin::Rect]`.
fn area_growth(lhs: &Rect, rhs: &Rect) -> f32 {
    let lhs_area = lhs.width * lhs.height;
    let rhs_area = rhs.width * rhs.height;
    1. - cmp::min(rhs_area, lhs_area) as f32 / cmp::max(rhs_area, lhs_area) as f32
}

/// Check if two given [`glutin::Rect`] overlap.
pub fn rects_overlap(lhs: &Rect, rhs: &Rect) -> bool {
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

/// Merge two [`glutin::Rect`] by producing the smallest rectangle that contains both.
pub fn merge_rects(lhs: Rect, rhs: Rect) -> Rect {
    let left_x = cmp::min(lhs.x, rhs.x);
    let right_x = cmp::max(lhs.x + lhs.width, rhs.x + rhs.width);
    let y_top = cmp::max(lhs.y + lhs.height, rhs.y + rhs.height);
    let y_bottom = cmp::min(lhs.y, rhs.y);
    Rect { x: left_x, y: y_bottom, width: right_x - left_x, height: y_top - y_bottom }
}
