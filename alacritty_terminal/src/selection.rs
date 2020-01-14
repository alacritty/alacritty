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
/// [`update`]: enum.Selection.html#method.update
#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    Simple {
        /// The region representing start and end of cursor movement
        region: Range<Anchor>,
    },
    Block {
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
    pub fn rotate(&mut self, offset: isize) {
        match *self {
            Selection::Simple { ref mut region } | Selection::Block { ref mut region } => {
                region.start.point.line += offset;
                region.end.point.line += offset;
            },
            Selection::Semantic { ref mut region } | Selection::Lines { ref mut region } => {
                region.start.line += offset;
                region.end.line += offset;
            },
        }
    }

    pub fn simple(location: Point<usize>, side: Side) -> Selection {
        Selection::Simple {
            region: Range {
                start: Anchor::new(location.into(), side),
                end: Anchor::new(location.into(), side),
            },
        }
    }

    pub fn block(location: Point<usize>, side: Side) -> Selection {
        Selection::Block {
            region: Range {
                start: Anchor::new(location.into(), side),
                end: Anchor::new(location.into(), side),
            },
        }
    }

    pub fn semantic(point: Point<usize>) -> Selection {
        Selection::Semantic { region: Range { start: point.into(), end: point.into() } }
    }

    pub fn lines(point: Point<usize>) -> Selection {
        Selection::Lines { region: Range { start: point.into(), end: point.into() } }
    }

    pub fn update(&mut self, location: Point<usize>, side: Side) {
        // Always update the `end`; can normalize later during span generation.
        match *self {
            Selection::Simple { ref mut region } | Selection::Block { ref mut region } => {
                region.end = Anchor::new(location.into(), side);
            },
            Selection::Semantic { ref mut region } | Selection::Lines { ref mut region } => {
                region.end = location.into();
            },
        }
    }

    pub fn is_empty(&self) -> bool {
        match *self {
            Selection::Simple { ref region } => {
                let (start, end) =
                    if Selection::points_need_swap(region.start.point, region.end.point) {
                        (&region.end, &region.start)
                    } else {
                        (&region.start, &region.end)
                    };

                // Simple selection is empty when the points are identical
                // or two adjacent cells have the sides right -> left
                start == end
                    || (start.side == Side::Right
                        && end.side == Side::Left
                        && (start.point.line == end.point.line)
                        && start.point.col + 1 == end.point.col)
            },
            Selection::Block { region: Range { ref start, ref end } } => {
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
            Selection::Semantic { .. } | Selection::Lines { .. } => false,
        }
    }

    pub fn to_span<T>(&self, term: &Term<T>) -> Option<Span> {
        // Get both sides of the selection
        let (mut start, mut end) = match *self {
            Selection::Simple { ref region } | Selection::Block { ref region } => {
                (region.start.point, region.end.point)
            },
            Selection::Semantic { ref region } | Selection::Lines { ref region } => {
                (region.start, region.end)
            },
        };

        // Order the start/end
        let needs_swap = Selection::points_need_swap(start, end);
        if needs_swap {
            std::mem::swap(&mut start, &mut end);
        }

        // Clamp to visible region in grid/normal
        let num_cols = term.dimensions().col;
        let num_lines = term.dimensions().line.0 as isize;
        let (start, end) = Selection::grid_clamp(start, end, num_lines, num_cols)?;

        let span = match *self {
            Selection::Simple { ref region } => {
                let (start_side, end_side) = if needs_swap {
                    (region.end.side, region.start.side)
                } else {
                    (region.start.side, region.end.side)
                };

                self.span_simple(term, start, end, start_side, end_side)
            },
            Selection::Block { ref region } => {
                let (start_side, end_side) = if needs_swap {
                    (region.end.side, region.start.side)
                } else {
                    (region.start.side, region.end.side)
                };

                self.span_block(start, end, start_side, end_side)
            },
            Selection::Semantic { .. } => Selection::span_semantic(term, start, end),
            Selection::Lines { .. } => Selection::span_lines(term, start, end),
        };

        // Expand selection across double-width cells
        span.map(|mut span| {
            let grid = term.grid();

            // Helper for checking if cell at `point` contains `flag`
            let flag_at = |point: Point<usize>, flag: Flags| -> bool {
                grid[point.line][point.col].flags.contains(flag)
            };

            // Include all double-width cells and placeholders at top left of selection
            if span.start.col < num_cols {
                // Expand from wide char spacer to wide char
                if span.start.line + 1 != grid.len() || span.start.col.0 != 0 {
                    let prev = span.start.sub(num_cols.0, 1, true);
                    if flag_at(span.start, Flags::WIDE_CHAR_SPACER)
                        && flag_at(prev, Flags::WIDE_CHAR)
                    {
                        span.start = prev;
                    }
                }

                // Expand from wide char to wide char spacer for linewrapping
                if span.start.line + 1 != grid.len() || span.start.col.0 != 0 {
                    let prev = span.start.sub(num_cols.0, 1, true);
                    if (prev.line + 1 != grid.len() || prev.col.0 != 0)
                        && flag_at(prev, Flags::WIDE_CHAR_SPACER)
                        && !flag_at(prev.sub(num_cols.0, 1, true), Flags::WIDE_CHAR)
                    {
                        span.start = prev;
                    }
                }
            }

            // Include all double-width cells and placeholders at bottom right of selection
            if span.end.line != 0 || span.end.col < num_cols {
                // Expand from wide char spacer for linewrapping to wide char
                if (span.end.line + 1 != grid.len() || span.end.col.0 != 0)
                    && flag_at(span.end, Flags::WIDE_CHAR_SPACER)
                    && !flag_at(span.end.sub(num_cols.0, 1, true), Flags::WIDE_CHAR)
                {
                    span.end = span.end.add(num_cols.0, 1, true);
                }

                // Expand from wide char to wide char spacer
                if flag_at(span.end, Flags::WIDE_CHAR) {
                    span.end = span.end.add(num_cols.0, 1, true);
                }
            }

            span
        })
    }

    // Bring start and end points in the correct order
    fn points_need_swap(start: Point<isize>, end: Point<isize>) -> bool {
        start.line < end.line || start.line == end.line && start.col > end.col
    }

    // Clamp selection inside the grid to prevent out of bounds errors
    fn grid_clamp(
        mut start: Point<isize>,
        mut end: Point<isize>,
        lines: isize,
        cols: Column,
    ) -> Option<(Point<isize>, Point<isize>)> {
        if start.line >= lines {
            // Don't show selection above visible region
            if end.line >= lines {
                return None;
            }

            // Clamp selection above viewport to visible region
            start.line = lines - 1;
            start.col = Column(0);
        }

        if end.line < 0 {
            // Don't show selection below visible region
            if start.line < 0 {
                return None;
            }

            // Clamp selection below viewport to visible region
            end.line = 0;
            end.col = cols - 1;
        }

        Some((start, end))
    }

    fn span_semantic<T>(term: &T, start: Point<isize>, end: Point<isize>) -> Option<Span>
    where
        T: Search + Dimensions,
    {
        let (start, end) = if start == end {
            if let Some(end) = term.bracket_search(start.into()) {
                (start.into(), end)
            } else {
                (term.semantic_search_left(start.into()), term.semantic_search_right(end.into()))
            }
        } else {
            (term.semantic_search_left(start.into()), term.semantic_search_right(end.into()))
        };

        Some(Span { start, end, is_block: false })
    }

    fn span_lines<T>(term: &T, mut start: Point<isize>, mut end: Point<isize>) -> Option<Span>
    where
        T: Search + Dimensions,
    {
        let needs_swap = Selection::points_need_swap(start, end);
        if needs_swap {
            start = term.search_wrapline_left(start.into()).into();
            end = term.search_wrapline_right(end.into()).into();
        } else {
            start = term.search_wrapline_right(start.into()).into();
            end = term.search_wrapline_left(end.into()).into();
        }

        start.col = term.dimensions().col - 1;
        end.col = Column(0);

        let (start, end) = (start.into(), end.into());

        Some(Span { start, end, is_block: false })
    }

    fn span_simple<T>(
        &self,
        term: &T,
        mut start: Point<isize>,
        mut end: Point<isize>,
        start_side: Side,
        end_side: Side,
    ) -> Option<Span>
    where
        T: Dimensions,
    {
        if self.is_empty() {
            return None;
        }

        // Remove last cell if selection ends to the left of a cell
        if end_side == Side::Left && start != end {
            // Special case when selection ends to left of first cell
            if end.col == Column(0) {
                end.col = term.dimensions().col - 1;
                end.line += 1;
            } else {
                end.col -= 1;
            }
        }

        // Remove first cell if selection starts at the right of a cell
        if start_side == Side::Right && start != end {
            start.col += 1;
        }

        // Return the selection with all cells inclusive
        Some(Span { start: start.into(), end: end.into(), is_block: false })
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

        // Always go top-left -> bottom-right
        if start.col > end.col {
            std::mem::swap(&mut start_side, &mut end_side);
            std::mem::swap(&mut start.col, &mut end.col);
        }

        // Remove last cell if selection ends to the left of a cell
        if end_side == Side::Left && start != end && end.col.0 > 0 {
            end.col -= 1;
        }

        // Remove first cell if selection starts at the right of a cell
        if start_side == Side::Right && start != end {
            start.col += 1;
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
    use crate::config::MockConfig;
    use crate::event::{Event, EventListener};
    use crate::grid::Grid;
    use crate::index::{Column, Line, Point, Side};
    use crate::term::cell::{Cell, Flags};
    use crate::term::{SizeInfo, Term};

    struct Mock;
    impl EventListener for Mock {
        fn send_event(&self, _event: Event) {}
    }

    fn term(width: usize, height: usize) -> Term<Mock> {
        let size = SizeInfo {
            width: width as f32,
            height: height as f32,
            cell_width: 1.0,
            cell_height: 1.0,
            padding_x: 0.0,
            padding_y: 0.0,
            dpr: 1.0,
        };
        Term::new(&MockConfig::default(), &size, Clipboard::new_nop(), Mock)
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
            is_block: false
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
            is_block: false
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
            start: Point::new(1, Column(2)),
            end: Point::new(0, Column(1)),
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
            start: Point::new(1, Column(1)),
            end: Point::new(0, Column(1)),
            is_block: false,
        });
    }

    #[test]
    fn line_selection() {
        let mut selection = Selection::lines(Point::new(0, Column(0)));
        selection.update(Point::new(5, Column(3)), Side::Right);
        selection.rotate(-3);

        assert_eq!(selection.to_span(&term(5, 10)).unwrap(), Span {
            start: Point::new(2, Column(0)),
            end: Point::new(0, Column(4)),
            is_block: false,
        });
    }

    #[test]
    fn semantic_selection() {
        let mut selection = Selection::semantic(Point::new(0, Column(0)));
        selection.update(Point::new(5, Column(3)), Side::Right);
        selection.rotate(-3);

        assert_eq!(selection.to_span(&term(5, 10)).unwrap(), Span {
            start: Point::new(2, Column(3)),
            end: Point::new(0, Column(4)),
            is_block: false,
        });
    }

    #[test]
    fn simple_selection() {
        let mut selection = Selection::simple(Point::new(0, Column(0)), Side::Right);
        selection.update(Point::new(5, Column(3)), Side::Right);
        selection.rotate(-3);

        assert_eq!(selection.to_span(&term(5, 10)).unwrap(), Span {
            start: Point::new(2, Column(4)),
            end: Point::new(0, Column(4)),
            is_block: false,
        });
    }

    #[test]
    fn block_selection() {
        let mut selection = Selection::block(Point::new(0, Column(0)), Side::Right);
        selection.update(Point::new(5, Column(3)), Side::Right);
        selection.rotate(-3);

        assert_eq!(selection.to_span(&term(5, 10)).unwrap(), Span {
            start: Point::new(2, Column(4)),
            end: Point::new(0, Column(4)),
            is_block: true
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
            start: Point::new(0, Column(0)),
            end: Point::new(0, Column(9)),
            is_block: false,
        });
    }

    #[test]
    fn simple_is_empty() {
        let mut selection = Selection::simple(Point::new(0, Column(0)), Side::Right);
        assert!(selection.is_empty());
        selection.update(Point::new(0, Column(1)), Side::Left);
        assert!(selection.is_empty());
        selection.update(Point::new(1, Column(0)), Side::Right);
        assert!(!selection.is_empty());
    }

    #[test]
    fn block_is_empty() {
        let mut selection = Selection::block(Point::new(0, Column(0)), Side::Right);
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
}
