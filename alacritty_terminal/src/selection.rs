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
use std::cmp::{max, min};
use std::ops::Range;

use crate::index::{Column, Point, Side};
use crate::term::cell::Flags;
use crate::term::{Search, Term};

/// Describes a region of a 2-dimensional area
///
/// Used to track a text selection. There are three supported modes, each with its own constructor:
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
pub enum Selection {
    Simple {
        /// The region representing start and end of cursor movement
        region: Range<Anchor>,
    },
    Semantic {
        /// The region representing start and end of cursor movement
        region: Range<Point<isize>>,
    },
    Lines {
        /// The region representing start and end of cursor movement
        region: Range<Point<isize>>,

        /// The line under the initial point. This is always selected regardless
        /// of which way the cursor is moved.
        initial_line: isize,
    },
}

/// A Point and side within that point.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchor {
    point: Point<isize>,
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
        Selection::Simple {
            region: Range {
                start: Anchor::new(location.into(), side),
                end: Anchor::new(location.into(), side),
            },
        }
    }

    pub fn rotate(&mut self, offset: isize) {
        match *self {
            Selection::Simple { ref mut region } => {
                region.start.point.line += offset;
                region.end.point.line += offset;
            },
            Selection::Semantic { ref mut region } => {
                region.start.line += offset;
                region.end.line += offset;
            },
            Selection::Lines { ref mut region, ref mut initial_line } => {
                region.start.line += offset;
                region.end.line += offset;
                *initial_line += offset;
            },
        }
    }

    pub fn semantic(point: Point<usize>) -> Selection {
        Selection::Semantic { region: Range { start: point.into(), end: point.into() } }
    }

    pub fn lines(point: Point<usize>) -> Selection {
        Selection::Lines {
            region: Range { start: point.into(), end: point.into() },
            initial_line: point.line as isize,
        }
    }

    pub fn update(&mut self, location: Point<usize>, side: Side) {
        // Always update the `end`; can normalize later during span generation.
        match *self {
            Selection::Simple { ref mut region } => {
                region.end = Anchor::new(location.into(), side);
            },
            Selection::Semantic { ref mut region } | Selection::Lines { ref mut region, .. } => {
                region.end = location.into();
            },
        }
    }

    pub fn to_span(&self, term: &Term, alt_screen: bool) -> Option<Span> {
        let span = match *self {
            Selection::Simple { ref region } => Selection::span_simple(term, region, alt_screen),
            Selection::Semantic { ref region } => {
                Selection::span_semantic(term, region, alt_screen)
            },
            Selection::Lines { ref region, initial_line } => {
                Selection::span_lines(term, region, initial_line, alt_screen)
            },
        };

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
        match *self {
            Selection::Simple { ref region } => {
                region.start == region.end && region.start.side == region.end.side
            },
            Selection::Semantic { .. } | Selection::Lines { .. } => false,
        }
    }

    fn span_semantic<G>(grid: &G, region: &Range<Point<isize>>, alt_screen: bool) -> Option<Span>
    where
        G: Search + Dimensions,
    {
        let cols = grid.dimensions().col;
        let lines = grid.dimensions().line.0 as isize;

        // Normalize ordering of selected cells
        let (mut start, mut end) = if region.start < region.end {
            (region.start, region.end)
        } else {
            (region.end, region.start)
        };

        if alt_screen {
            Selection::alt_screen_clamp(&mut start, &mut end, lines, cols)?;
        }

        let (mut start, mut end) = if start == end {
            if let Some(end) = grid.bracket_search(start.into()) {
                (start.into(), end)
            } else {
                (grid.semantic_search_right(start.into()), grid.semantic_search_left(end.into()))
            }
        } else if start < end && start.line == end.line {
            (grid.semantic_search_left(start.into()), grid.semantic_search_right(end.into()))
        } else {
            (grid.semantic_search_right(start.into()), grid.semantic_search_left(end.into()))
        };

        if start > end {
            std::mem::swap(&mut start, &mut end);
        }

        Some(Span { start, end })
    }

    fn span_lines<G>(
        grid: &G,
        region: &Range<Point<isize>>,
        initial_line: isize,
        alt_screen: bool,
    ) -> Option<Span>
    where
        G: Dimensions,
    {
        let cols = grid.dimensions().col;
        let lines = grid.dimensions().line.0 as isize;

        // First, create start and end points based on initial line and the grid
        // dimensions.
        let mut start = Point { col: cols - 1, line: initial_line };
        let mut end = Point { col: Column(0), line: initial_line };

        // Now, expand lines based on where cursor started and ended.
        if region.start.line < region.end.line {
            // Start is below end
            start.line = min(start.line, region.start.line);
            end.line = max(end.line, region.end.line);
        } else {
            // Start is above end
            start.line = min(start.line, region.end.line);
            end.line = max(end.line, region.start.line);
        }

        if alt_screen {
            Selection::alt_screen_clamp(&mut start, &mut end, lines, cols)?;
        }

        Some(Span { start: start.into(), end: end.into() })
    }

    fn span_simple<G>(grid: &G, region: &Range<Anchor>, alt_screen: bool) -> Option<Span>
    where
        G: Dimensions,
    {
        let start = region.start.point;
        let start_side = region.start.side;
        let end = region.end.point;
        let end_side = region.end.side;
        let cols = grid.dimensions().col;
        let lines = grid.dimensions().line.0 as isize;

        // Make sure start is always the "bottom" and end is always the "top"
        let (mut start, mut end, start_side, end_side) =
            if start.line > end.line || start.line == end.line && start.col <= end.col {
                // Selected upward; start/end are swapped
                (end, start, end_side, start_side)
            } else {
                // Selected downward; no swapping
                (start, end, start_side, end_side)
            };

        // No selection for single cell with identical sides or two cell with right+left sides
        if (start == end && start_side == end_side)
            || (end_side == Side::Right
                && start_side == Side::Left
                && start.line == end.line
                && start.col == end.col + 1)
        {
            return None;
        }

        // Remove last cell if selection ends to the left of a cell
        if start_side == Side::Left && start != end {
            // Special case when selection starts to left of first cell
            if start.col == Column(0) {
                start.col = cols - 1;
                start.line += 1;
            } else {
                start.col -= 1;
            }
        }

        // Remove first cell if selection starts at the right of a cell
        if end_side == Side::Right && start != end {
            end.col += 1;
        }

        if alt_screen {
            Selection::alt_screen_clamp(&mut start, &mut end, lines, cols)?;
        }

        // Return the selection with all cells inclusive
        Some(Span { start: start.into(), end: end.into() })
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
