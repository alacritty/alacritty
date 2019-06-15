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
use std::mem::swap;
use std::ops::Range;

use crate::index::{Column, Point, Side};
use crate::term::cell::Flags;
use crate::term::{Search, Term};

/// Describes a type of a 2-dimensional area
///
/// Used to track a text selection type. There are three supported modes, each with its own constructor:
/// [`simple`], [`semantic`], and [`lines`]. The [`simple`] mode precisely tracks which cells are
/// selected without any expansion. [`semantic`] mode expands the initial selection to the nearest
/// semantic escape char in either direction. [`lines`] will always select entire lines.
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
/// Used to track the selection. The selection can type can be heterogeneous
/// where each end of the region can be a different selection type.
///
/// Calls to [`update`] operate different based on the selection kind. The [`simple`] mode does
/// nothing special, simply tracks points and sides. [`semantic`] will continue to expand out to
/// semantic boundaries as the selection point changes. Similarly, [`lines`] will always expand the
/// new point to encompass entire lines.
///
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    /// The region representing start and end of the cursor movement
    pub region: Range<Anchor>,

    start_type: SelectionType,
    end_type: SelectionType,
}

/// A Point and side within that point.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    pub point: Point<isize>,
    side: Side,
}

impl Anchor {
    fn new(point: Point<isize>, side: Side) -> Anchor {
        Anchor { point, side }
    }
}

/// A type that has 2-dimensional boundaries
pub trait Dimensions {
    /// Get the size of the area
    fn dimensions(&self) -> Point;
}

impl Selection {
    pub fn simple(location: Point<usize>, side: Side) -> Selection {
        Selection {
            region: Range {
                start: Anchor::new(location.into(), side),
                end: Anchor::new(location.into(), side),
            },
            start_type: SelectionType::Simple,
            end_type: SelectionType::Simple,
        }
    }

    pub fn reverse(&mut self) {
        swap(&mut self.region.start, &mut self.region.end);
        swap(&mut self.start_type, &mut self.end_type);
    }

    pub fn rotate(&mut self, offset: isize) {
        self.region.start.point.line += offset;
        self.region.end.point.line += offset;
    }

    pub fn semantic(point: Point<usize>) -> Selection {
        Selection {
            region: Range {
                start: Anchor::new(point.into(), Side::Right),
                end: Anchor::new(point.into(), Side::Right),
            },
            start_type: SelectionType::Semantic,
            end_type: SelectionType::Semantic,
        }
    }

    pub fn lines(point: Point<usize>) -> Selection {
        Selection {
            region: Range {
                start: Anchor::new(point.into(), Side::Right),
                end: Anchor::new(point.into(), Side::Right),
            },
            start_type: SelectionType::Lines,
            end_type: SelectionType::Lines,
        }
    }

    pub fn update(&mut self, location: Point<usize>, side: Side) {
        // Always update the `end`; can normalize later during span generation.
        // Always set side; can be ignored later if needed.
        self.region.end = Anchor::new(location.into(), side);
    }

    pub fn update_as(&mut self, location: Point<usize>, side: Side, sel_type: SelectionType) {
        self.update(location, side);
        self.end_type = sel_type;
    }

    // Expand selection following the similar rules as creating a span
    pub fn expand_selection(term: &mut Term) {
        let (mut start_anchor, mut end_anchor, mut start_type, mut end_type) =
            if let Some(ref selection) = term.selection() {
            (selection.region.start.clone(),
                selection.region.end.clone(),
                selection.start_type.clone(),
                selection.end_type.clone())
        } else {
            return;
        };

        let expansion = |start: &mut Anchor,
            end: &mut Anchor,
            start_type: &mut SelectionType,
            end_type: &mut SelectionType|
            {
                // semantic selection
                if *start_type == SelectionType::Semantic {
                    start.point = Point::from(term.semantic_search_right(start.point.into()));
                    start.side = Side::Right;
                    *start_type = SelectionType::Simple;
                }
                if *end_type == SelectionType::Semantic {
                    end.point = Point::from(term.semantic_search_left(end.point.into()));
                    end.side = Side::Left;
                    *end_type = SelectionType::Simple;
                }

                // Line expansion
                let cols = term.dimensions().col;
                if *start_type == SelectionType::Lines {
                    start.point = Point { line: start.point.line, col: cols - 1 };
                    start.side = Side::Right;
                    *start_type = SelectionType::Simple;
                }
                if *end_type == SelectionType::Lines {
                    end.point = Point { line: end.point.line, col: Column(0) };
                    end.side = Side::Left;
                    *end_type = SelectionType::Simple;
            }

            (start.clone(), end.clone(), start_type.clone(), end_type.clone())
        };

        if start_anchor.point.line > end_anchor.point.line
            || start_anchor.point.line == end_anchor.point.line
                && start_anchor.point.col <= end_anchor.point.col
            {
                let (end_anchor, start_anchor, end_type, start_type) =
                    expansion(&mut end_anchor, &mut start_anchor, &mut end_type, &mut start_type);
                if let Some(ref mut selection) = term.selection_mut() {
                    selection.region.start.point = start_anchor.point;
                    selection.region.start.side = start_anchor.side;
                    selection.region.end.point = end_anchor.point;
                    selection.start_type = start_type;
                    selection.end_type = end_type;
            }
        } else {
            let (start_anchor, end_anchor, start_type, end_type) =
                expansion(&mut start_anchor, &mut end_anchor, &mut start_type, &mut end_type);
            if let Some(ref mut selection) = term.selection_mut() {
                selection.region.start.point = start_anchor.point;
                selection.region.end.point = end_anchor.point;
                selection.region.end.side = end_anchor.side;
                selection.start_type = start_type;
                selection.end_type = end_type;
            }
        }
    }

    pub fn to_span(&self, term: &Term, alt_screen: bool) -> Option<Span> {
        let start_orig = self.region.start.point;
        let start_side = self.region.start.side;
        let end_orig = self.region.end.point;
        let end_side = self.region.end.side;
        let cols = term.dimensions().col;
        let lines = term.dimensions().line.0 as isize;
        let (mut start, mut end, start_side, end_side, start_type, end_type) =
            if start_orig.line > end_orig.line || start_orig.line == end_orig.line && start_orig.col <= end_orig.col {
                (end_orig, start_orig, end_side, start_side, self.end_type.clone(), self.start_type.clone())
            } else {
                (start_orig, end_orig, start_side, end_side, self.start_type.clone(), self.end_type.clone())
            };
        
        if alt_screen {
            Selection::alt_screen_clamp(&mut start, &mut end, lines, cols)?;
        }

        // Simple Selection
        // No selection for single cell with identical sides or two cell with right+left sides
        if start_type == SelectionType::Simple
            && end_type == SelectionType::Simple
            && ((start == end
            && start_side == end_side)
            || (end_side == Side::Right
                && start_side == Side::Left
                && start.line == end.line
                && start.col == end.col + 1))
        {
            return None;
        }
        // Remove first cell if selection starts to the left of a cell
        if start_side == Side::Left && start != end && start_type == SelectionType::Simple {
            // Special case when selection starts to left of first cell
            if start.col == Column(0) {
                start.col = cols - 1;
                start.line += 1;
            } else {
                start.col -= 1;
            }
        }
        // Remove last cell if selection ends at the right of a cell
        if end_side == Side::Right && start != end && end_type == SelectionType::Simple {
            end.col += 1;
        }

        // Semantic expansion
        if start_type == SelectionType::Semantic {
            start = Point::from(term.semantic_search_right(start.into()));
        }
        if end_type == SelectionType::Semantic {
            end = Point::from(term.semantic_search_left(end.into()));
        }

        // Line expansion
        if start_type == SelectionType::Lines {
            start = Point { line: start.line, col: cols - 1 };
        }
        if end_type == SelectionType::Lines {
            end = Point { line: end.line, col: Column(0) };
        }

        let span = Some(Span { start: start.into(), end: end.into() });

        // Expand selection across double-width cells
        span.map(|mut span| {
            let grid = term.grid();

            if span.end.col < grid.num_cols()
                && grid[span.end.line][span.end.col].flags.contains(Flags::WIDE_CHAR_SPACER)
            {
                span.end.col = Column(span.end.col.saturating_sub(1));
            }

            if span.start.col.0 < grid.num_cols().saturating_sub(1)
                && grid[span.start.line][span.start.col].flags.contains(Flags::WIDE_CHAR)
            {
                span.start.col += 1;
            }

            span
        })
    }

    pub fn is_empty(&self) -> bool {
        if self.start_type == SelectionType::Simple && self.end_type == SelectionType::Simple {
            self.region.start == self.region.end
        } else {
            false
        }
    }

    // Clamp selection in the alternate screen to the visible region
    fn alt_screen_clamp(
        start: &mut Point<isize>,
        end: &mut Point<isize>,
        lines: isize,
        cols: Column,
    ) -> Option<()> {
        if end.line >= lines {
            // Don't show selection above visible region
            if start.line >= lines {
                return None;
            }

            // Clamp selection above viewport to visible region
            end.line = lines - 1;
            end.col = Column(0);
        }

        if start.line < 0 {
            // Don't show selection below visible region
            if end.line < 0 {
                return None;
            }

            // Clamp selection below viewport to visible region
            start.line = 0;
            start.col = cols - 1;
        }

        Some(())
    }
}

/// Represents a span of selected cells
#[derive(Debug, Eq, PartialEq)]
pub struct Span {
    /// Start point from bottom of buffer
    pub start: Point<usize>,
    /// End point towards top of buffer
    pub end: Point<usize>,
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

    use super::{Anchor, Selection, SelectionType, Span};
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

        assert_eq!(selection.to_span(&term(1, 1), false).unwrap(), Span {
            start: location,
            end: location
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

        assert_eq!(selection.to_span(&term(1, 1), false).unwrap(), Span {
            start: location,
            end: location
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

        assert_eq!(selection.to_span(&term(2, 1), false), None);
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

        assert_eq!(selection.to_span(&term(2, 1), false), None);
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

        assert_eq!(selection.to_span(&term(5, 2), false).unwrap(), Span {
            start: Point::new(0, Column(1)),
            end: Point::new(1, Column(2)),
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

        assert_eq!(selection.to_span(&term(5, 2), false).unwrap(), Span {
            start: Point::new(0, Column(1)),
            end: Point::new(1, Column(1)),
        });
    }

    /// Test reverse lines selection
    ///
    /// 1.  [BX][XX][XX][XX][XX]
    ///     [XX][XX][XX][XX][XE]
    /// 2.  [EX][XX][XX][XX][XX]
    ///     [XX][XX][XX][XX][XB]
    #[test]
    fn reverse_lines() {
        let mut selection = Selection::lines(Point::new(0, Column(1)));
        selection.update(Point::new(1, Column(3)), Side::Left);
        selection.reverse();

        assert_eq!(selection, Selection {
            region: Anchor { point: Point { line: 1, col: Column(3) }, side: Side::Left }
                .. Anchor { point: Point { line: 0, col: Column(1) }, side: Side::Right },
            start_type: SelectionType::Lines,
            end_type: SelectionType::Lines,
        });
    }

    /// Test reverse semantic selection
    ///
    /// 1.  [  ][ B][XX][XX][XX]
    ///     [XX][XX][XX][E ][  ]
    /// 2.  [  ][ E][XX][XX][XX]
    ///     [XX][XX][XX][B ][  ]
    #[test]
    fn reverse_semantic() {
        let mut selection = Selection::semantic(Point::new(0, Column(1)));
        selection.update(Point::new(1, Column(3)), Side::Left);
        selection.reverse();

        assert_eq!(selection, Selection {
            region: Anchor { point: Point { line: 1, col: Column(3) }, side: Side::Left }
                .. Anchor { point: Point { line: 0, col: Column(1) }, side: Side::Right },
            start_type: SelectionType::Semantic,
            end_type: SelectionType::Semantic,
        });
    }

    /// Test reverse simple selection
    ///
    /// 1.  [  ][ B][XX][XX][XX]
    ///     [XX][XX][XX][E ][  ]
    /// 2.  [  ][ E][XX][XX][XX]
    ///     [XX][XX][XX][B ][  ]
    #[test]
    fn reverse_simple() {
        let mut selection = Selection::simple(Point::new(0, Column(1)), Side::Right);
        selection.update(Point::new(1, Column(3)), Side::Left);
        selection.reverse();

        assert_eq!(selection, Selection {
            region: Anchor { point: Point { line: 1, col: Column(3) }, side: Side::Left }
                .. Anchor { point: Point { line: 0, col: Column(1) }, side: Side::Right },
            start_type: SelectionType::Simple,
            end_type: SelectionType::Simple,
        });
    }

    #[test]
    fn alt_scren_lines() {
        let mut selection = Selection::lines(Point::new(0, Column(0)));
        selection.update(Point::new(5, Column(3)), Side::Right);
        selection.rotate(-3);

        assert_eq!(selection.to_span(&term(5, 10), true).unwrap(), Span {
            start: Point::new(0, Column(4)),
            end: Point::new(2, Column(0)),
        });
    }

    #[test]
    fn alt_screen_semantic() {
        let mut selection = Selection::semantic(Point::new(0, Column(0)));
        selection.update(Point::new(5, Column(3)), Side::Right);
        selection.rotate(-3);

        assert_eq!(selection.to_span(&term(5, 10), true).unwrap(), Span {
            start: Point::new(0, Column(4)),
            end: Point::new(2, Column(3)),
        });
    }

    #[test]
    fn alt_screen_simple() {
        let mut selection = Selection::simple(Point::new(0, Column(0)), Side::Right);
        selection.update(Point::new(5, Column(3)), Side::Right);
        selection.rotate(-3);

        assert_eq!(selection.to_span(&term(5, 10), true).unwrap(), Span {
            start: Point::new(0, Column(4)),
            end: Point::new(2, Column(4)),
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

        assert_eq!(selection.to_span(&term, false).unwrap(), Span {
            start: Point::new(0, Column(9)),
            end: Point::new(0, Column(0)),
        });
    }
}
