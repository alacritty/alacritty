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
use crate::term::Search;

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

    pub fn to_span<G>(&self, grid: &G, alt_screen: bool) -> Option<Span>
    where
        G: Search + Dimensions,
    {
        match *self {
            Selection::Simple { ref region } => Selection::span_simple(grid, region, alt_screen),
            Selection::Semantic { ref region } => {
                Selection::span_semantic(grid, region, alt_screen)
            },
            Selection::Lines { ref region, initial_line } => {
                Selection::span_lines(grid, region, initial_line, alt_screen)
            },
        }
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
        let (mut front, mut tail) = if region.start < region.end {
            (region.start, region.end)
        } else {
            (region.end, region.start)
        };

        if alt_screen {
            Selection::alt_screen_clamp(&mut front, &mut tail, lines, cols)?;
        }

        let (mut start, mut end) = if front < tail && front.line == tail.line {
            (grid.semantic_search_left(front.into()), grid.semantic_search_right(tail.into()))
        } else {
            (grid.semantic_search_right(front.into()), grid.semantic_search_left(tail.into()))
        };

        if start > end {
            ::std::mem::swap(&mut start, &mut end);
        }

        Some(Span { cols, front: start, tail: end, ty: SpanType::Inclusive })
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

        Some(Span { cols, front: start.into(), tail: end.into(), ty: SpanType::Inclusive })
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

        // Make sure front is always the "bottom" and tail is always the "top"
        let (mut front, mut tail, front_side, tail_side) =
            if start.line > end.line || start.line == end.line && start.col <= end.col {
                // Selected upward; start/end are swapped
                (end, start, end_side, start_side)
            } else {
                // Selected downward; no swapping
                (start, end, start_side, end_side)
            };

        // No selection for single cell with identical sides or two cell with right+left sides
        if (front == tail && front_side == tail_side)
            || (tail_side == Side::Right
                && front_side == Side::Left
                && front.line == tail.line
                && front.col == tail.col + 1)
        {
            return None;
        }

        // Remove last cell if selection ends to the left of a cell
        if front_side == Side::Left && start != end {
            // Special case when selection starts to left of first cell
            if front.col == Column(0) {
                front.col = cols - 1;
                front.line += 1;
            } else {
                front.col -= 1;
            }
        }

        // Remove first cell if selection starts at the right of a cell
        if tail_side == Side::Right && front != tail {
            tail.col += 1;
        }

        if alt_screen {
            Selection::alt_screen_clamp(&mut front, &mut tail, lines, cols)?;
        }

        // Return the selection with all cells inclusive
        Some(Span { cols, front: front.into(), tail: tail.into(), ty: SpanType::Inclusive })
    }

    // Clamp selection in the alternate screen to the visible region
    fn alt_screen_clamp(
        front: &mut Point<isize>,
        tail: &mut Point<isize>,
        lines: isize,
        cols: Column,
    ) -> Option<()> {
        if tail.line >= lines {
            // Don't show selection above visible region
            if front.line >= lines {
                return None;
            }

            // Clamp selection above viewport to visible region
            tail.line = lines - 1;
            tail.col = Column(0);
        }

        if front.line < 0 {
            // Don't show selection below visible region
            if tail.line < 0 {
                return None;
            }

            // Clamp selection below viewport to visible region
            front.line = 0;
            front.col = cols - 1;
        }

        Some(())
    }
}

/// How to interpret the locations of a Span.
#[derive(Debug, Eq, PartialEq)]
pub enum SpanType {
    /// Includes the beginning and end locations
    Inclusive,

    /// Exclude both beginning and end
    Exclusive,

    /// Excludes last cell of selection
    ExcludeTail,

    /// Excludes first cell of selection
    ExcludeFront,
}

/// Represents a span of selected cells
#[derive(Debug, Eq, PartialEq)]
pub struct Span {
    front: Point<usize>,
    tail: Point<usize>,
    cols: Column,

    /// The type says whether ends are included or not.
    ty: SpanType,
}

#[derive(Debug)]
pub struct Locations {
    /// Start point from bottom of buffer
    pub start: Point<usize>,
    /// End point towards top of buffer
    pub end: Point<usize>,
}

impl Span {
    pub fn to_locations(&self) -> Locations {
        let (start, end) = match self.ty {
            SpanType::Inclusive => (self.front, self.tail),
            SpanType::Exclusive => {
                (Span::wrap_start(self.front, self.cols), Span::wrap_end(self.tail, self.cols))
            },
            SpanType::ExcludeFront => (Span::wrap_start(self.front, self.cols), self.tail),
            SpanType::ExcludeTail => (self.front, Span::wrap_end(self.tail, self.cols)),
        };

        Locations { start, end }
    }

    fn wrap_start(mut start: Point<usize>, cols: Column) -> Point<usize> {
        if start.col == cols - 1 {
            Point { line: start.line + 1, col: Column(0) }
        } else {
            start.col += 1;
            start
        }
    }

    fn wrap_end(end: Point<usize>, cols: Column) -> Point<usize> {
        if end.col == Column(0) && end.line != 0 {
            Point { line: end.line - 1, col: cols }
        } else {
            Point { line: end.line, col: end.col - 1 }
        }
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
    use super::{Selection, Span, SpanType};
    use crate::index::{Column, Line, Point, Side};
    use crate::url::Url;

    struct Dimensions(Point);
    impl super::Dimensions for Dimensions {
        fn dimensions(&self) -> Point {
            self.0
        }
    }

    impl Dimensions {
        pub fn new(line: usize, col: usize) -> Self {
            Dimensions(Point { line: Line(line), col: Column(col) })
        }
    }

    impl super::Search for Dimensions {
        fn semantic_search_left(&self, point: Point<usize>) -> Point<usize> {
            point
        }

        fn semantic_search_right(&self, point: Point<usize>) -> Point<usize> {
            point
        }

        fn url_search(&self, _: Point<usize>) -> Option<Url> {
            None
        }
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

        assert_eq!(selection.to_span(&Dimensions::new(1, 1), false).unwrap(), Span {
            cols: Column(1),
            ty: SpanType::Inclusive,
            front: location,
            tail: location
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

        assert_eq!(selection.to_span(&Dimensions::new(1, 1), false).unwrap(), Span {
            cols: Column(1),
            ty: SpanType::Inclusive,
            front: location,
            tail: location
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

        assert_eq!(selection.to_span(&Dimensions::new(1, 2), false), None);
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

        assert_eq!(selection.to_span(&Dimensions::new(1, 2), false), None);
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

        assert_eq!(selection.to_span(&Dimensions::new(2, 5), false).unwrap(), Span {
            cols: Column(5),
            front: Point::new(0, Column(1)),
            tail: Point::new(1, Column(2)),
            ty: SpanType::Inclusive,
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

        assert_eq!(selection.to_span(&Dimensions::new(2, 5), false).unwrap(), Span {
            cols: Column(5),
            front: Point::new(0, Column(1)),
            tail: Point::new(1, Column(1)),
            ty: SpanType::Inclusive,
        });
    }

    #[test]
    fn alt_scren_lines() {
        let mut selection = Selection::lines(Point::new(0, Column(0)));
        selection.update(Point::new(5, Column(3)), Side::Right);
        selection.rotate(-3);

        assert_eq!(selection.to_span(&Dimensions::new(10, 5), true).unwrap(), Span {
            cols: Column(5),
            front: Point::new(0, Column(4)),
            tail: Point::new(2, Column(0)),
            ty: SpanType::Inclusive,
        });
    }

    #[test]
    fn alt_screen_semantic() {
        let mut selection = Selection::semantic(Point::new(0, Column(0)));
        selection.update(Point::new(5, Column(3)), Side::Right);
        selection.rotate(-3);

        assert_eq!(selection.to_span(&Dimensions::new(10, 5), true).unwrap(), Span {
            cols: Column(5),
            front: Point::new(0, Column(4)),
            tail: Point::new(2, Column(3)),
            ty: SpanType::Inclusive,
        });
    }

    #[test]
    fn alt_screen_simple() {
        let mut selection = Selection::simple(Point::new(0, Column(0)), Side::Right);
        selection.update(Point::new(5, Column(3)), Side::Right);
        selection.rotate(-3);

        assert_eq!(selection.to_span(&Dimensions::new(10, 5), true).unwrap(), Span {
            cols: Column(5),
            front: Point::new(0, Column(4)),
            tail: Point::new(2, Column(4)),
            ty: SpanType::Inclusive,
        });
    }
}
