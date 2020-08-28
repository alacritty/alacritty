//! State management for a selection in the grid.
//!
//! A selection should start when the mouse is clicked, and it should be
//! finalized when the button is released. The selection should be cleared
//! when text is added/removed/scrolled on the screen. The selection should
//! also be cleared if the user clicks off of the selection.

use std::convert::TryFrom;
use std::mem;
use std::ops::{Bound, Range, RangeBounds};

use crate::grid::Dimensions;
use crate::index::{Column, Line, Point, Side};
use crate::term::Term;

/// A Point and side within that point.
#[derive(Debug, Copy, Clone, PartialEq)]
struct Anchor {
    point: Point<usize>,
    side: Side,
}

impl Anchor {
    fn new(point: Point<usize>, side: Side) -> Anchor {
        Anchor { point, side }
    }
}

/// Represents a range of selected cells.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SelectionRange<L = usize> {
    /// Start point, top left of the selection.
    pub start: Point<L>,
    /// End point, bottom right of the selection.
    pub end: Point<L>,
    /// Whether this selection is a block selection.
    pub is_block: bool,
}

impl<L> SelectionRange<L> {
    pub fn new(start: Point<L>, end: Point<L>, is_block: bool) -> Self {
        Self { start, end, is_block }
    }

    pub fn contains(&self, col: Column, line: L) -> bool
    where
        L: PartialEq + PartialOrd,
    {
        self.start.line <= line
            && self.end.line >= line
            && (self.start.col <= col || (self.start.line != line && !self.is_block))
            && (self.end.col >= col || (self.end.line != line && !self.is_block))
    }
}

/// Different kinds of selection.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum SelectionType {
    Simple,
    Block,
    Semantic,
    Lines,
}

/// Describes a region of a 2-dimensional area.
///
/// Used to track a text selection. There are four supported modes, each with its own constructor:
/// [`simple`], [`block`], [`semantic`], and [`lines`]. The [`simple`] mode precisely tracks which
/// cells are selected without any expansion. [`block`] will select rectangular regions.
/// [`semantic`] mode expands the initial selection to the nearest semantic escape char in either
/// direction. [`lines`] will always select entire lines.
///
/// Calls to [`update`] operate different based on the selection kind. The [`simple`] and [`block`]
/// mode do nothing special, simply track points and sides. [`semantic`] will continue to expand
/// out to semantic boundaries as the selection point changes. Similarly, [`lines`] will always
/// expand the new point to encompass entire lines.
///
/// [`simple`]: enum.Selection.html#method.simple
/// [`block`]: enum.Selection.html#method.block
/// [`semantic`]: enum.Selection.html#method.semantic
/// [`lines`]: enum.Selection.html#method.lines
/// [`update`]: enum.Selection.html#method.update
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    pub ty: SelectionType,
    region: Range<Anchor>,
}

impl Selection {
    pub fn new(ty: SelectionType, location: Point<usize>, side: Side) -> Selection {
        Self {
            region: Range { start: Anchor::new(location, side), end: Anchor::new(location, side) },
            ty,
        }
    }

    /// Update the end of the selection.
    pub fn update(&mut self, point: Point<usize>, side: Side) {
        self.region.end = Anchor::new(point, side);
    }

    pub fn rotate<D: Dimensions>(
        mut self,
        dimensions: &D,
        range: &Range<Line>,
        delta: isize,
    ) -> Option<Selection> {
        let num_lines = dimensions.screen_lines().0;
        let num_cols = dimensions.cols().0;
        let range_bottom = range.start.0;
        let range_top = range.end.0;

        let (mut start, mut end) = (&mut self.region.start, &mut self.region.end);
        if Selection::points_need_swap(start.point, end.point) {
            mem::swap(&mut start, &mut end);
        }

        // Rotate start of selection.
        if (start.point.line < range_top || range_top == num_lines)
            && start.point.line >= range_bottom
        {
            start.point.line = usize::try_from(start.point.line as isize + delta).unwrap_or(0);

            // If end is within the same region, delete selection once start rotates out.
            if start.point.line < range_bottom && end.point.line >= range_bottom {
                return None;
            }

            // Clamp selection to start of region.
            if start.point.line >= range_top && range_top != num_lines {
                if self.ty != SelectionType::Block {
                    start.point.col = Column(0);
                    start.side = Side::Left;
                }
                start.point.line = range_top - 1;
            }
        }

        // Rotate end of selection.
        if (end.point.line < range_top || range_top == num_lines) && end.point.line >= range_bottom
        {
            end.point.line = usize::try_from(end.point.line as isize + delta).unwrap_or(0);

            // Delete selection if end has overtaken the start.
            if end.point.line > start.point.line {
                return None;
            }

            // Clamp selection to end of region.
            if end.point.line < range_bottom {
                if self.ty != SelectionType::Block {
                    end.point.col = Column(num_cols - 1);
                    end.side = Side::Right;
                }
                end.point.line = range_bottom;
            }
        }

        Some(self)
    }

    pub fn is_empty(&self) -> bool {
        match self.ty {
            SelectionType::Simple => {
                let (mut start, mut end) = (self.region.start, self.region.end);
                if Self::points_need_swap(start.point, end.point) {
                    mem::swap(&mut start, &mut end);
                }

                // Simple selection is empty when the points are identical
                // or two adjacent cells have the sides right -> left.
                start == end
                    || (start.side == Side::Right
                        && end.side == Side::Left
                        && (start.point.line == end.point.line)
                        && start.point.col + 1 == end.point.col)
            },
            SelectionType::Block => {
                let (start, end) = (self.region.start, self.region.end);

                // Block selection is empty when the points' columns and sides are identical
                // or two cells with adjacent columns have the sides right -> left,
                // regardless of their lines
                (start.point.col == end.point.col && start.side == end.side)
                    || (start.point.col + 1 == end.point.col
                        && start.side == Side::Right
                        && end.side == Side::Left)
                    || (end.point.col + 1 == start.point.col
                        && start.side == Side::Left
                        && end.side == Side::Right)
            },
            SelectionType::Semantic | SelectionType::Lines => false,
        }
    }

    /// Check whether selection contains any point in a given range.
    pub fn intersects_range<R: RangeBounds<usize>>(&self, range: R) -> bool {
        let mut start = self.region.start.point.line;
        let mut end = self.region.end.point.line;

        if Self::points_need_swap(self.region.start.point, self.region.end.point) {
            mem::swap(&mut start, &mut end);
        }

        let range_start = match range.start_bound() {
            Bound::Included(&range_start) => range_start,
            Bound::Excluded(&range_start) => range_start.saturating_add(1),
            Bound::Unbounded => 0,
        };

        let range_end = match range.end_bound() {
            Bound::Included(&range_end) => range_end,
            Bound::Excluded(&range_end) => range_end.saturating_sub(1),
            Bound::Unbounded => usize::max_value(),
        };

        range_start <= start && range_end >= end
    }

    /// Expand selection sides to include all cells.
    pub fn include_all(&mut self) {
        let (start, end) = (self.region.start.point, self.region.end.point);
        let (start_side, end_side) = match self.ty {
            SelectionType::Block
                if start.col > end.col || (start.col == end.col && start.line < end.line) =>
            {
                (Side::Right, Side::Left)
            },
            SelectionType::Block => (Side::Left, Side::Right),
            _ if Self::points_need_swap(start, end) => (Side::Right, Side::Left),
            _ => (Side::Left, Side::Right),
        };

        self.region.start.side = start_side;
        self.region.end.side = end_side;
    }

    /// Convert selection to grid coordinates.
    pub fn to_range<T>(&self, term: &Term<T>) -> Option<SelectionRange> {
        let grid = term.grid();
        let num_cols = grid.cols();

        // Order start above the end.
        let mut start = self.region.start;
        let mut end = self.region.end;

        if Self::points_need_swap(start.point, end.point) {
            mem::swap(&mut start, &mut end);
        }

        // Clamp to inside the grid buffer.
        let is_block = self.ty == SelectionType::Block;
        let (start, end) = Self::grid_clamp(start, end, is_block, grid.total_lines()).ok()?;

        match self.ty {
            SelectionType::Simple => self.range_simple(start, end, num_cols),
            SelectionType::Block => self.range_block(start, end),
            SelectionType::Semantic => Self::range_semantic(term, start.point, end.point),
            SelectionType::Lines => Self::range_lines(term, start.point, end.point),
        }
    }

    /// Bring start and end points in the correct order.
    fn points_need_swap(start: Point<usize>, end: Point<usize>) -> bool {
        start.line < end.line || start.line == end.line && start.col > end.col
    }

    /// Clamp selection inside grid to prevent OOB.
    fn grid_clamp(
        mut start: Anchor,
        end: Anchor,
        is_block: bool,
        lines: usize,
    ) -> Result<(Anchor, Anchor), ()> {
        // Clamp selection inside of grid to prevent OOB.
        if start.point.line >= lines {
            // Remove selection if it is fully out of the grid.
            if end.point.line >= lines {
                return Err(());
            }

            // Clamp to grid if it is still partially visible.
            if !is_block {
                start.side = Side::Left;
                start.point.col = Column(0);
            }
            start.point.line = lines - 1;
        }

        Ok((start, end))
    }

    fn range_semantic<T>(
        term: &Term<T>,
        mut start: Point<usize>,
        mut end: Point<usize>,
    ) -> Option<SelectionRange> {
        if start == end {
            if let Some(matching) = term.bracket_search(start) {
                if (matching.line == start.line && matching.col < start.col)
                    || (matching.line > start.line)
                {
                    start = matching;
                } else {
                    end = matching;
                }

                return Some(SelectionRange { start, end, is_block: false });
            }
        }

        start = term.semantic_search_left(start);
        end = term.semantic_search_right(end);

        Some(SelectionRange { start, end, is_block: false })
    }

    fn range_lines<T>(
        term: &Term<T>,
        mut start: Point<usize>,
        mut end: Point<usize>,
    ) -> Option<SelectionRange> {
        start = term.line_search_left(start);
        end = term.line_search_right(end);

        Some(SelectionRange { start, end, is_block: false })
    }

    fn range_simple(
        &self,
        mut start: Anchor,
        mut end: Anchor,
        num_cols: Column,
    ) -> Option<SelectionRange> {
        if self.is_empty() {
            return None;
        }

        // Remove last cell if selection ends to the left of a cell.
        if end.side == Side::Left && start.point != end.point {
            // Special case when selection ends to left of first cell.
            if end.point.col == Column(0) {
                end.point.col = num_cols - 1;
                end.point.line += 1;
            } else {
                end.point.col -= 1;
            }
        }

        // Remove first cell if selection starts at the right of a cell.
        if start.side == Side::Right && start.point != end.point {
            start.point.col += 1;

            // Wrap to next line when selection starts to the right of last column.
            if start.point.col == num_cols {
                start.point = Point::new(start.point.line.saturating_sub(1), Column(0));
            }
        }

        Some(SelectionRange { start: start.point, end: end.point, is_block: false })
    }

    fn range_block(&self, mut start: Anchor, mut end: Anchor) -> Option<SelectionRange> {
        if self.is_empty() {
            return None;
        }

        // Always go top-left -> bottom-right.
        if start.point.col > end.point.col {
            mem::swap(&mut start.side, &mut end.side);
            mem::swap(&mut start.point.col, &mut end.point.col);
        }

        // Remove last cell if selection ends to the left of a cell.
        if end.side == Side::Left && start.point != end.point && end.point.col.0 > 0 {
            end.point.col -= 1;
        }

        // Remove first cell if selection starts at the right of a cell.
        if start.side == Side::Right && start.point != end.point {
            start.point.col += 1;
        }

        Some(SelectionRange { start: start.point, end: end.point, is_block: true })
    }
}

/// Tests for selection.
///
/// There are comments on all of the tests describing the selection. Pictograms
/// are used to avoid ambiguity. Grid cells are represented by a [  ]. Only
/// cells that are completely covered are counted in a selection. Ends are
/// represented by `B` and `E` for begin and end, respectively.  A selected cell
/// looks like [XX], [BX] (at the start), [XB] (at the end), [XE] (at the end),
/// and [EX] (at the start), or [BE] for a single cell. Partially selected cells
/// look like [ B] and [E ].
#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::MockConfig;
    use crate::event::{Event, EventListener};
    use crate::index::{Column, Line, Point, Side};
    use crate::term::{SizeInfo, Term};

    struct Mock;
    impl EventListener for Mock {
        fn send_event(&self, _event: Event) {}
    }

    fn term(height: usize, width: usize) -> Term<Mock> {
        let size = SizeInfo {
            width: width as f32,
            height: height as f32,
            cell_width: 1.0,
            cell_height: 1.0,
            padding_x: 0.0,
            padding_y: 0.0,
            dpr: 1.0,
        };
        Term::new(&MockConfig::default(), size, Mock)
    }

    /// Test case of single cell selection.
    ///
    /// 1. [  ]
    /// 2. [B ]
    /// 3. [BE]
    #[test]
    fn single_cell_left_to_right() {
        let location = Point { line: 0, col: Column(0) };
        let mut selection = Selection::new(SelectionType::Simple, location, Side::Left);
        selection.update(location, Side::Right);

        assert_eq!(selection.to_range(&term(1, 1)).unwrap(), SelectionRange {
            start: location,
            end: location,
            is_block: false
        });
    }

    /// Test case of single cell selection.
    ///
    /// 1. [  ]
    /// 2. [ B]
    /// 3. [EB]
    #[test]
    fn single_cell_right_to_left() {
        let location = Point { line: 0, col: Column(0) };
        let mut selection = Selection::new(SelectionType::Simple, location, Side::Right);
        selection.update(location, Side::Left);

        assert_eq!(selection.to_range(&term(1, 1)).unwrap(), SelectionRange {
            start: location,
            end: location,
            is_block: false
        });
    }

    /// Test adjacent cell selection from left to right.
    ///
    /// 1. [  ][  ]
    /// 2. [ B][  ]
    /// 3. [ B][E ]
    #[test]
    fn between_adjacent_cells_left_to_right() {
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(0, Column(0)), Side::Right);
        selection.update(Point::new(0, Column(1)), Side::Left);

        assert_eq!(selection.to_range(&term(1, 2)), None);
    }

    /// Test adjacent cell selection from right to left.
    ///
    /// 1. [  ][  ]
    /// 2. [  ][B ]
    /// 3. [ E][B ]
    #[test]
    fn between_adjacent_cells_right_to_left() {
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(0, Column(1)), Side::Left);
        selection.update(Point::new(0, Column(0)), Side::Right);

        assert_eq!(selection.to_range(&term(1, 2)), None);
    }

    /// Test selection across adjacent lines.
    ///
    /// 1.  [  ][  ][  ][  ][  ]
    ///     [  ][  ][  ][  ][  ]
    /// 2.  [  ][ B][  ][  ][  ]
    ///     [  ][  ][  ][  ][  ]
    /// 3.  [  ][ B][XX][XX][XX]
    ///     [XX][XE][  ][  ][  ]
    #[test]
    fn across_adjacent_lines_upward_final_cell_exclusive() {
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(1, Column(1)), Side::Right);
        selection.update(Point::new(0, Column(1)), Side::Right);

        assert_eq!(selection.to_range(&term(2, 5)).unwrap(), SelectionRange {
            start: Point::new(1, Column(2)),
            end: Point::new(0, Column(1)),
            is_block: false,
        });
    }

    /// Test selection across adjacent lines.
    ///
    /// 1.  [  ][  ][  ][  ][  ]
    ///     [  ][  ][  ][  ][  ]
    /// 2.  [  ][  ][  ][  ][  ]
    ///     [  ][ B][  ][  ][  ]
    /// 3.  [  ][ E][XX][XX][XX]
    ///     [XX][XB][  ][  ][  ]
    /// 4.  [ E][XX][XX][XX][XX]
    ///     [XX][XB][  ][  ][  ]
    #[test]
    fn selection_bigger_then_smaller() {
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(0, Column(1)), Side::Right);
        selection.update(Point::new(1, Column(1)), Side::Right);
        selection.update(Point::new(1, Column(0)), Side::Right);

        assert_eq!(selection.to_range(&term(2, 5)).unwrap(), SelectionRange {
            start: Point::new(1, Column(1)),
            end: Point::new(0, Column(1)),
            is_block: false,
        });
    }

    #[test]
    fn line_selection() {
        let size = (Line(10), Column(5));
        let mut selection =
            Selection::new(SelectionType::Lines, Point::new(0, Column(1)), Side::Left);
        selection.update(Point::new(5, Column(1)), Side::Right);
        selection = selection.rotate(&size, &(Line(0)..size.0), 7).unwrap();

        assert_eq!(selection.to_range(&term(*size.0, *size.1)).unwrap(), SelectionRange {
            start: Point::new(9, Column(0)),
            end: Point::new(7, Column(4)),
            is_block: false,
        });
    }

    #[test]
    fn semantic_selection() {
        let size = (Line(10), Column(5));
        let mut selection =
            Selection::new(SelectionType::Semantic, Point::new(0, Column(3)), Side::Left);
        selection.update(Point::new(5, Column(1)), Side::Right);
        selection = selection.rotate(&size, &(Line(0)..size.0), 7).unwrap();

        assert_eq!(selection.to_range(&term(*size.0, *size.1)).unwrap(), SelectionRange {
            start: Point::new(9, Column(0)),
            end: Point::new(7, Column(3)),
            is_block: false,
        });
    }

    #[test]
    fn simple_selection() {
        let size = (Line(10), Column(5));
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(0, Column(3)), Side::Right);
        selection.update(Point::new(5, Column(1)), Side::Right);
        selection = selection.rotate(&size, &(Line(0)..size.0), 7).unwrap();

        assert_eq!(selection.to_range(&term(*size.0, *size.1)).unwrap(), SelectionRange {
            start: Point::new(9, Column(0)),
            end: Point::new(7, Column(3)),
            is_block: false,
        });
    }

    #[test]
    fn block_selection() {
        let size = (Line(10), Column(5));
        let mut selection =
            Selection::new(SelectionType::Block, Point::new(0, Column(3)), Side::Right);
        selection.update(Point::new(5, Column(1)), Side::Right);
        selection = selection.rotate(&size, &(Line(0)..size.0), 7).unwrap();

        assert_eq!(selection.to_range(&term(*size.0, *size.1)).unwrap(), SelectionRange {
            start: Point::new(9, Column(2)),
            end: Point::new(7, Column(3)),
            is_block: true
        });
    }

    #[test]
    fn simple_is_empty() {
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(0, Column(0)), Side::Right);
        assert!(selection.is_empty());
        selection.update(Point::new(0, Column(1)), Side::Left);
        assert!(selection.is_empty());
        selection.update(Point::new(1, Column(0)), Side::Right);
        assert!(!selection.is_empty());
    }

    #[test]
    fn block_is_empty() {
        let mut selection =
            Selection::new(SelectionType::Block, Point::new(0, Column(0)), Side::Right);
        assert!(selection.is_empty());
        selection.update(Point::new(0, Column(1)), Side::Left);
        assert!(selection.is_empty());
        selection.update(Point::new(0, Column(1)), Side::Right);
        assert!(!selection.is_empty());
        selection.update(Point::new(1, Column(0)), Side::Right);
        assert!(selection.is_empty());
        selection.update(Point::new(1, Column(1)), Side::Left);
        assert!(selection.is_empty());
        selection.update(Point::new(1, Column(1)), Side::Right);
        assert!(!selection.is_empty());
    }

    #[test]
    fn rotate_in_region_up() {
        let size = (Line(10), Column(5));
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(2, Column(3)), Side::Right);
        selection.update(Point::new(5, Column(1)), Side::Right);
        selection = selection.rotate(&size, &(Line(1)..(size.0 - 1)), 4).unwrap();

        assert_eq!(selection.to_range(&term(*size.0, *size.1)).unwrap(), SelectionRange {
            start: Point::new(8, Column(0)),
            end: Point::new(6, Column(3)),
            is_block: false,
        });
    }

    #[test]
    fn rotate_in_region_down() {
        let size = (Line(10), Column(5));
        let mut selection =
            Selection::new(SelectionType::Simple, Point::new(5, Column(3)), Side::Right);
        selection.update(Point::new(8, Column(1)), Side::Left);
        selection = selection.rotate(&size, &(Line(1)..(size.0 - 1)), -5).unwrap();

        assert_eq!(selection.to_range(&term(*size.0, *size.1)).unwrap(), SelectionRange {
            start: Point::new(3, Column(1)),
            end: Point::new(1, size.1 - 1),
            is_block: false,
        });
    }

    #[test]
    fn rotate_in_region_up_block() {
        let size = (Line(10), Column(5));
        let mut selection =
            Selection::new(SelectionType::Block, Point::new(2, Column(3)), Side::Right);
        selection.update(Point::new(5, Column(1)), Side::Right);
        selection = selection.rotate(&size, &(Line(1)..(size.0 - 1)), 4).unwrap();

        assert_eq!(selection.to_range(&term(*size.0, *size.1)).unwrap(), SelectionRange {
            start: Point::new(8, Column(2)),
            end: Point::new(6, Column(3)),
            is_block: true,
        });
    }

    #[test]
    fn range_intersection() {
        let mut selection =
            Selection::new(SelectionType::Lines, Point::new(6, Column(1)), Side::Left);
        selection.update(Point::new(3, Column(1)), Side::Right);

        assert!(selection.intersects_range(..));
        assert!(selection.intersects_range(2..));
        assert!(selection.intersects_range(2..=4));
        assert!(selection.intersects_range(2..=7));
        assert!(selection.intersects_range(4..=5));
        assert!(selection.intersects_range(5..8));

        assert!(!selection.intersects_range(..=2));
        assert!(!selection.intersects_range(7..=8));
    }
}
