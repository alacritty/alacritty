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

//! State management for a selection in the grid
//!
//! A selection should start when the mouse is clicked, and it should be
//! finalized when the button is released. The selection should be cleared
//! when text is added/removed/scrolled on the screen. The selection should
//! also be cleared if the user clicks off of the selection.
use std::ops::Range;

use crate::index::{Column, Line, Point, Side};
use crate::term::cell::Flags;
use crate::term::{Search, Term};

/// Describes a type of Selection
///
/// Used to track a text selection type. There are three supported modes, each with its own constructor:
/// [`simple`], [`semantic`], and [`lines`]. The [`simple`] mode precisely tracks which cells are
/// selected without any expansion. [`semantic`] mode expands the initial selection to the nearest
/// semantic escape char in either direction. [`lines`] will always select entire lines.
///
/// Calls to [`update`] operate different based on the selection kind. The [`simple`] mode does
/// nothing special, simply tracks points and sides. [`semantic`] will continue to expand out to
/// semantic boundaries as the selection point changes. Similarly, [`lines`] will always expand the
/// new point to encompass entire lines.
///
/// [`simple`]: enum.Selection.html#method.simple
/// [`semantic`]: enum.Selection.html#method.semantic
/// [`lines`]: enum.Selection.html#method.lines
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionType {
    Simple,
    Semantic,
    Lines,
}

/// Describes a region of a 2-dimensional area
/// 
/// Used to track the selection. The selection type can be heterogeneous
/// where each end of the region can be a different selection type.
#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    Normal {
        /// The region representing start and end of cursor movement
        region: Range<Anchor>,
    },
    Block {
        /// The region representing start and end of cursor movement
        region: Range<Anchor>,
    },
}

/// A Point and side within that point.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    pub point: Point<isize>,
    pub ty: SelectionType,
    side: Side,
}

impl Anchor {
    fn new(point: Point<isize>, ty: SelectionType, side: Side) -> Anchor {
        Anchor { point, ty, side }
    }
}

/// A type that has 2-dimensional boundaries
pub trait Dimensions {
    /// Get the size of the area
    fn dimensions(&self) -> Point;
}

impl Selection {
    pub fn rotate(&mut self, offset: isize) {
        match *self {
            Selection::Normal { ref mut region } | Selection::Block { ref mut region } => {
                region.start.point.line += offset;
                region.end.point.line += offset;
            },
        }
    }

    pub fn simple(location: Point<usize>, side: Side) -> Selection {
        Selection::Normal {
            region: Range {
                start: Anchor::new(location.into(), SelectionType::Simple, side),
                end: Anchor::new(location.into(), SelectionType::Simple, side),
            },
        }
    }

    pub fn block(location: Point<usize>, side: Side) -> Selection {
        Selection::Block {
            region: Range {
                start: Anchor::new(location.into(), SelectionType::Simple, side),
                end: Anchor::new(location.into(), SelectionType::Simple, side),
            },
        }
    }

    pub fn semantic(point: Point<usize>) -> Selection {
        Selection::Normal {
            region: Range {
                start: Anchor::new(point.into(), SelectionType::Semantic, Side::Right),
                end: Anchor::new(point.into(), SelectionType::Semantic, Side::Right),
            },
        }
    }

    pub fn lines(point: Point<usize>) -> Selection {
        Selection::Normal {
            region: Range {
                start: Anchor::new(point.into(), SelectionType::Lines, Side::Right),
                end: Anchor::new(point.into(), SelectionType::Lines, Side::Right),
            },
        }
    }

    pub fn update(&mut self, location: Point<usize>, side: Side) {
        match *self {
            Selection::Normal {ref mut region } | Selection::Block {ref mut region } => {
                // Always update the `end`; can normalize later during span generation.
                // Always set side; can be ignored later if needed.
                region.end = Anchor::new(location.into(), region.end.ty.clone(), side);
            }
        }
    }

    pub fn update_as(&mut self, location: Point<usize>, side: Side, selection_type: SelectionType) {
        match *self {
            Selection::Normal {ref mut region } | Selection::Block { ref mut region } => {
                region.end.ty = selection_type;
            }
        }
        self.update(location, side);
    }

    // Expand selection following the similar rules as creating a span
    pub fn expand_selection(term: &mut Term) {
        let (mut start_anchor, mut end_anchor) =
            if let Some(ref selection) = term.selection() {
                match selection {
                    Selection::Normal { ref region } => {
                        (region.start.clone(),
                            region.end.clone())
                    }
                    _ => { return; }
                }
            } else {
                return;
            };

        let expansion = |start: &mut Anchor, end: &mut Anchor|
            {
                // semantic selection
                if start.ty == SelectionType::Semantic {
                    start.point = Point::from(term.semantic_search_right(start.point.into()));
                    start.side = Side::Right;
                    start.ty = SelectionType::Simple;
                }
                if end.ty == SelectionType::Semantic {
                    end.point = Point::from(term.semantic_search_left(end.point.into()));
                    end.side = Side::Left;
                    end.ty = SelectionType::Simple;
                }

                // Line expansion
                let cols = term.dimensions().col;
                if start.ty == SelectionType::Lines {
                    start.point = Point { line: start.point.line, col: cols - 1 };
                    start.side = Side::Right;
                    start.ty = SelectionType::Simple;
                }
                if end.ty == SelectionType::Lines {
                    end.point = Point { line: end.point.line, col: Column(0) };
                    end.side = Side::Left;
                    end.ty = SelectionType::Simple;
            }

            (start.clone(), end.clone())
        };

        if Selection::points_need_swap(start_anchor.point, end_anchor.point)
        {
            let (end_anchor, start_anchor) =
                expansion(&mut end_anchor, &mut start_anchor);
            if let Some(ref mut selection) = term.selection_mut() {
                match selection {
                    Selection::Normal { ref mut region } => {
                        region.start = start_anchor;
                        region.end = end_anchor;
                    }
                    _ => { return; }
                }
            }
        } else {
            let (start_anchor, end_anchor) =
                expansion(&mut start_anchor, &mut end_anchor);
            if let Some(ref mut selection) = term.selection_mut() {
                match selection {
                    Selection::Normal { ref mut region } => {
                        region.start = start_anchor;
                        region.end = end_anchor;
                    }
                    _ => { return; }
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        match *self {
            Selection::Normal { ref region } | Selection::Block { ref region } => {
                let (start, end) =
                    if Selection::points_need_swap(region.start.point, region.end.point) {
                        (&region.end, &region.start)
                    } else {
                        (&region.start, &region.end)
                    };

                if region.start.ty != SelectionType::Simple {
                    return false;
                }

                // Empty when single cell with identical sides or two cell with right+left sides
                start == end
                    || (start.side == Side::Left
                        && end.side == Side::Right
                        && start.point.line == end.point.line
                        && start.point.col == end.point.col + 1)
            },
        }
    }

    pub fn to_span(&self, term: &Term) -> Option<Span> {
        // Get both sides of the selection
        let (mut start, mut end) = match self {
            Selection::Normal { region } | Selection::Block { region } => {
                (region.start.clone(), region.end.clone())
            },
        };
        
        // Order the start/end
        let needs_swap = Selection::points_need_swap(start.point, end.point);
        if needs_swap {
            std::mem::swap(&mut start, &mut end);
        }

        // Clamp to visible region in grid/normal
        let cols = term.dimensions().col;
        let lines = term.dimensions().line.0 as isize;
        let (start, end) = Selection::grid_clamp(start, end, lines, cols)?;

        let span = match *self {
            Selection::Normal { .. } => {
                // Simple Selection
                let (start, end): (Anchor, Anchor) = self.span_simple(cols, start, end)?;

                // Semantic expansion
                let (start, end) = Selection::span_semantic(term, start, end);

                // Line expansion
                let (start, end) = Selection::span_line(cols, start, end);

                Some(Span { start: start.point.into(), end: end.point.into(), is_block: false })
            }
            Selection::Block {ref region} => {
                let (start_side, end_side) = if needs_swap {
                    (region.end.side, region.start.side)
                } else {
                    (region.start.side, region.end.side)
                };

                self.span_block(start.point, end.point, start_side, end_side)
            }
        };

        // Expand selection across double-width cells
        span.map(|mut span| {
            let grid = term.grid();

            if span.end.col < cols
                && grid[span.end.line][span.end.col].flags.contains(Flags::WIDE_CHAR_SPACER)
            {
                span.end.col = Column(span.end.col.saturating_sub(1));
            }

            if span.start.col.0 < cols.saturating_sub(1)
                && grid[span.start.line][span.start.col].flags.contains(Flags::WIDE_CHAR)
            {
                span.start.col += 1;
            }

            span
        })
    }

    // Bring start and end points in the correct order
    fn points_need_swap(start: Point<isize>, end: Point<isize>) -> bool {
        start.line > end.line || start.line == end.line && start.col < end.col
    }

    // Clamp selection inside the grid to prevent out of bounds errors
    fn grid_clamp(
        mut start: Anchor,
        mut end: Anchor,
        lines: isize,
        cols: Column,
    ) -> Option<(Anchor, Anchor)> {
        if end.point.line >= lines {
            // Don't show selection above visible region
            if start.point.line >= lines {
                return None;
            }

            // Clamp selection above viewport to visible region
            end.point.line = lines - 1;
            end.point.col = Column(0);
        }

        if start.point.line < 0 {
            // Don't show selection below visible region
            if end.point.line < 0 {
                return None;
            }

            // Clamp selection below viewport to visible region
            start.point.line = 0;
            start.point.col = cols - 1;
        }

        Some((start, end))
    }

    fn span_semantic(term: &Term, mut start: Anchor, mut end: Anchor) -> (Anchor, Anchor) {
        if start.ty == SelectionType::Semantic {
            start.point = Point::from(term.semantic_search_right(start.point.into()));
        }
        if end.ty == SelectionType::Semantic {
            end.point = Point::from(term.semantic_search_left(end.point.into()));
        }
        (start, end)
    }

    fn span_line(cols: Column, mut start: Anchor, mut end: Anchor) -> (Anchor, Anchor) {
        if start.ty == SelectionType::Lines {
            start.point = Point { line: start.point.line, col: cols - 1 };
        }
        if end.ty == SelectionType::Lines {
            end.point = Point { line: end.point.line, col: Column(0) };
        }
        (start, end)
    }

    fn span_simple(&self, cols: Column, mut start: Anchor, mut end: Anchor) -> Option<(Anchor, Anchor)> {
        if self.is_empty() {
            return None;
        }
        // Remove first cell if selection starts to the left of a cell
        if start.side == Side::Left && start.point != end.point && start.ty == SelectionType::Simple {
            // Special case when selection starts to left of first cell
            if start.point.col == Column(0) {
                start.point.col = cols - 1;
                start.point.line += 1;
            } else {
                start.point.col -= 1;
            }
        }
        // Remove last cell if selection ends at the right of a cell
        if end.side == Side::Right && start.point != end.point && end.ty == SelectionType::Simple {
            end.point.col += 1;
        }
        Some((start, end))
    }

    fn span_block(
        &self,
        mut start: Point<isize>,
        mut end: Point<isize>,
        mut start_side: Side,
        mut end_side: Side,
    ) -> Option<Span> {
        if self.is_empty() {
            return None;
        }

        // Always go bottom-right -> top-left
        if start.col < end.col {
            std::mem::swap(&mut start_side, &mut end_side);
            std::mem::swap(&mut start.col, &mut end.col);
        }

        // Remove last cell if selection ends to the left of a cell
        if start_side == Side::Left && start != end && start.col.0 > 0 {
            start.col -= 1;
        }

        // Remove first cell if selection starts at the right of a cell
        if end_side == Side::Right && start != end {
            end.col += 1;
        }

        // Return the selection with all cells inclusive
        Some(Span { start: start.into(), end: end.into(), is_block: true })
    }
}

/// Represents a span of selected cells
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Span {
    /// Start point from bottom of buffer
    pub start: Point<usize>,
    /// End point towards top of buffer
    pub end: Point<usize>,
    /// Whether this selection is a block selection
    pub is_block: bool,
}

pub struct SelectionRange {
    start: Point,
    end: Point,
    is_block: bool,
}

impl SelectionRange {
    pub fn new(start: Point, end: Point, is_block: bool) -> Self {
        Self { start, end, is_block }
    }

    pub fn contains(&self, col: Column, line: Line) -> bool {
        self.start.line <= line
            && self.end.line >= line
            && (self.start.col <= col || (self.start.line != line && !self.is_block))
            && (self.end.col >= col || (self.end.line != line && !self.is_block))
    }
}

/// Tests for selection
///
/// There are comments on all of the tests describing the selection. Pictograms
/// are used to avoid ambiguity. Grid cells are represented by a [  ]. Only
/// cells that are completely covered are counted in a selection. Ends are
/// represented by `B` and `E` for begin and end, respectively.  A selected cell
/// looks like [XX], [BX] (at the start), [XB] (at the end), [XE] (at the end),
/// and [EX] (at the start), or [BE] for a single cell. Partially selected cells
/// look like [ B] and [E ].
#[cfg(test)]
mod test {
    use std::mem;

    use super::{Selection, Span};
    use crate::clipboard::Clipboard;
    use crate::grid::Grid;
    use crate::index::{Column, Line, Point, Side};
    use crate::message_bar::MessageBuffer;
    use crate::term::cell::{Cell, Flags};
    use crate::term::{SizeInfo, Term};

    fn term(width: usize, height: usize) -> Term {
        let size = SizeInfo {
            width: width as f32,
            height: height as f32,
            cell_width: 1.0,
            cell_height: 1.0,
            padding_x: 0.0,
            padding_y: 0.0,
            dpr: 1.0,
        };
        Term::new(&Default::default(), size, MessageBuffer::new(), Clipboard::new_nop())
    }

    /// Test case of single cell selection
    ///
    /// 1. [  ]
    /// 2. [B ]
    /// 3. [BE]
    #[test]
    fn single_cell_left_to_right() {
        let location = Point { line: 0, col: Column(0) };
        let mut selection = Selection::simple(location, Side::Left);
        selection.update(location, Side::Right);

        assert_eq!(selection.to_span(&term(1, 1)).unwrap(), Span {
            start: location,
            end: location,
            is_block: false,
        });
    }

    /// Test case of single cell selection
    ///
    /// 1. [  ]
    /// 2. [ B]
    /// 3. [EB]
    #[test]
    fn single_cell_right_to_left() {
        let location = Point { line: 0, col: Column(0) };
        let mut selection = Selection::simple(location, Side::Right);
        selection.update(location, Side::Left);

        assert_eq!(selection.to_span(&term(1, 1)).unwrap(), Span {
            start: location,
            end: location,
            is_block: false,
        });
    }

    /// Test adjacent cell selection from left to right
    ///
    /// 1. [  ][  ]
    /// 2. [ B][  ]
    /// 3. [ B][E ]
    #[test]
    fn between_adjacent_cells_left_to_right() {
        let mut selection = Selection::simple(Point::new(0, Column(0)), Side::Right);
        selection.update(Point::new(0, Column(1)), Side::Left);

        assert_eq!(selection.to_span(&term(2, 1)), None);
    }

    /// Test adjacent cell selection from right to left
    ///
    /// 1. [  ][  ]
    /// 2. [  ][B ]
    /// 3. [ E][B ]
    #[test]
    fn between_adjacent_cells_right_to_left() {
        let mut selection = Selection::simple(Point::new(0, Column(1)), Side::Left);
        selection.update(Point::new(0, Column(0)), Side::Right);

        assert_eq!(selection.to_span(&term(2, 1)), None);
    }

    /// Test selection across adjacent lines
    ///
    ///
    /// 1.  [  ][  ][  ][  ][  ]
    ///     [  ][  ][  ][  ][  ]
    /// 2.  [  ][ B][  ][  ][  ]
    ///     [  ][  ][  ][  ][  ]
    /// 3.  [  ][ B][XX][XX][XX]
    ///     [XX][XE][  ][  ][  ]
    #[test]
    fn across_adjacent_lines_upward_final_cell_exclusive() {
        let mut selection = Selection::simple(Point::new(1, Column(1)), Side::Right);
        selection.update(Point::new(0, Column(1)), Side::Right);

        assert_eq!(selection.to_span(&term(5, 2)).unwrap(), Span {
            start: Point::new(0, Column(1)),
            end: Point::new(1, Column(2)),
            is_block: false,
        });
    }

    /// Test selection across adjacent lines
    ///
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
        let mut selection = Selection::simple(Point::new(0, Column(1)), Side::Right);
        selection.update(Point::new(1, Column(1)), Side::Right);
        selection.update(Point::new(1, Column(0)), Side::Right);

        assert_eq!(selection.to_span(&term(5, 2)).unwrap(), Span {
            start: Point::new(0, Column(1)),
            end: Point::new(1, Column(1)),
            is_block: false,
        });
    }

    #[test]
    fn line_selection() {
        let mut selection = Selection::lines(Point::new(0, Column(0)));
        selection.update(Point::new(5, Column(3)), Side::Right);
        selection.rotate(-3);

        assert_eq!(selection.to_span(&term(5, 10)).unwrap(), Span {
            start: Point::new(0, Column(4)),
            end: Point::new(2, Column(0)),
            is_block: false,
        });
    }

    #[test]
    fn semantic_selection() {
        let mut selection = Selection::semantic(Point::new(0, Column(0)));
        selection.update(Point::new(5, Column(3)), Side::Right);
        selection.rotate(-3);

        assert_eq!(selection.to_span(&term(5, 10)).unwrap(), Span {
            start: Point::new(0, Column(4)),
            end: Point::new(2, Column(3)),
            is_block: false,
        });
    }

    #[test]
    fn simple_selection() {
        let mut selection = Selection::simple(Point::new(0, Column(0)), Side::Right);
        selection.update(Point::new(5, Column(3)), Side::Right);
        selection.rotate(-3);

        assert_eq!(selection.to_span(&term(5, 10)).unwrap(), Span {
            start: Point::new(0, Column(4)),
            end: Point::new(2, Column(4)),
            is_block: false,
        });
    }

    #[test]
    fn block_selection() {
        let mut selection = Selection::block(Point::new(0, Column(0)), Side::Right);
        selection.update(Point::new(5, Column(3)), Side::Right);
        selection.rotate(-3);

        assert_eq!(selection.to_span(&term(5, 10)).unwrap(), Span {
            start: Point::new(0, Column(4)),
            end: Point::new(2, Column(4)),
            is_block: true,
        });
    }

    #[test]
    fn double_width_expansion() {
        let mut term = term(10, 1);
        let mut grid = Grid::new(Line(1), Column(10), 0, Cell::default());
        grid[Line(0)][Column(0)].flags.insert(Flags::WIDE_CHAR);
        grid[Line(0)][Column(1)].flags.insert(Flags::WIDE_CHAR_SPACER);
        grid[Line(0)][Column(8)].flags.insert(Flags::WIDE_CHAR);
        grid[Line(0)][Column(9)].flags.insert(Flags::WIDE_CHAR_SPACER);
        mem::swap(term.grid_mut(), &mut grid);

        let mut selection = Selection::simple(Point::new(0, Column(1)), Side::Left);
        selection.update(Point::new(0, Column(8)), Side::Right);

        assert_eq!(selection.to_span(&term).unwrap(), Span {
            start: Point::new(0, Column(9)),
            end: Point::new(0, Column(0)),
            is_block: false,
        });
    }
}
