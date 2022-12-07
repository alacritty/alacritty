use std::cmp;
use std::iter::Peekable;

use glutin::surface::Rect;

use alacritty_terminal::term::{LineDamageBounds, TermDamageIterator};

use crate::display::SizeInfo;

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
        Rect::new(x as i32, y as i32, width as i32, size_info.cell_height() as i32)
    }

    // Make sure to damage near cells to include wide chars.
    #[inline]
    fn overdamage(size_info: &SizeInfo<u32>, mut rect: Rect) -> Rect {
        rect.x = (rect.x - size_info.cell_width() as i32).max(0);
        rect.width = cmp::min(
            size_info.width() as i32 - rect.x,
            rect.width + 2 * size_info.cell_width() as i32,
        );
        rect.y = (rect.y - size_info.cell_height() as i32 / 2).max(0);
        rect.height = cmp::min(
            size_info.height() as i32 - rect.y,
            rect.height + size_info.cell_height() as i32,
        );

        rect
    }
}

impl<'a> Iterator for RenderDamageIterator<'a> {
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
    }
}
