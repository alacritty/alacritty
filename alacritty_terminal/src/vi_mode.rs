use std::cmp::min;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::event::EventListener;
use crate::grid::{Dimensions, GridCell};
use crate::index::{Boundary, Column, Direction, Line, Point, Side};
use crate::term::cell::Flags;
use crate::term::Term;

/// Possible vi mode motion movements.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "lowercase"))]
pub enum ViMotion {
    /// Move up.
    Up,
    /// Move down.
    Down,
    /// Move left.
    Left,
    /// Move right.
    Right,
    /// First column, or beginning of the line when already at the first column.
    First,
    /// Last column, or beginning of the line when already at the last column.
    Last,
    /// First non-empty cell in this terminal row, or first non-empty cell
    /// of the line when already at the first cell of the row.
    FirstOccupied,
    /// Move to top of screen.
    High,
    /// Move to center of screen.
    Middle,
    /// Move to bottom of screen.
    Low,
    /// Move to start of semantically separated word.
    SemanticLeft,
    /// Move to start of next semantically separated word.
    SemanticRight,
    /// Move to end of previous semantically separated word.
    SemanticLeftEnd,
    /// Move to end of semantically separated word.
    SemanticRightEnd,
    /// Move to start of whitespace separated word.
    WordLeft,
    /// Move to start of next whitespace separated word.
    WordRight,
    /// Move to end of previous whitespace separated word.
    WordLeftEnd,
    /// Move to end of whitespace separated word.
    WordRightEnd,
    /// Move to opposing bracket.
    Bracket,
}

/// Cursor tracking vi mode position.
#[derive(Default, Copy, Clone, PartialEq, Eq)]
pub struct ViModeCursor {
    pub point: Point,
}

impl ViModeCursor {
    pub fn new(point: Point) -> Self {
        Self { point }
    }

    /// Move vi mode cursor `count` times and apply it to `term`.
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn motion<T: EventListener>(
        mut self,
        term: &mut Term<T>,
        motion: ViMotion,
        count: usize,
    ) -> Self {
        for _ in 0..count {
            let new_point: Point = Self::motion_impl(self.point, term, motion);
            if self.point == new_point {
                break;
            }
            self.point = new_point;
        }
        term.scroll_to_point(self.point);
        self
    }

    /// Move vi mode cursor.
    #[must_use = "this returns the result of the operation, without modifying the original"]
    fn motion_impl<T: EventListener>(mut point: Point, term: &Term<T>, motion: ViMotion) -> Point {
        match motion {
            ViMotion::Up => {
                if point.line > term.topmost_line() {
                    point.line -= 1;
                }
            },
            ViMotion::Down => {
                if point.line + 1 < term.screen_lines() {
                    point.line += 1;
                }
            },
            ViMotion::Left => {
                point = term.expand_wide(point, Direction::Left);
                let wrap_point = Point::new(point.line - 1, term.last_column());
                if point.column == 0
                    && point.line > term.topmost_line()
                    && is_wrap(term, wrap_point)
                {
                    point = wrap_point;
                } else {
                    point.column = Column(point.column.saturating_sub(1));
                }
            },
            ViMotion::Right => {
                point = term.expand_wide(point, Direction::Right);
                if is_wrap(term, point) {
                    point = Point::new(point.line + 1, Column(0));
                } else {
                    point.column = min(point.column + 1, term.last_column());
                }
            },
            ViMotion::First => {
                point = term.expand_wide(point, Direction::Left);
                while point.column == 0
                    && point.line > term.topmost_line()
                    && is_wrap(term, Point::new(point.line - 1, term.last_column()))
                {
                    point.line -= 1;
                }
                point.column = Column(0);
            },
            ViMotion::Last => point = last(term, point),
            ViMotion::FirstOccupied => point = first_occupied(term, point),
            ViMotion::High => {
                let display_offset = term.grid().display_offset() as i32;

                let line = Line(-display_offset);
                let col = first_occupied_in_line(term, line).unwrap_or_default().column;
                point = Point::new(line, col);
            },
            ViMotion::Middle => {
                let display_offset = term.grid().display_offset() as i32;
                let screen_lines = term.screen_lines() as i32;

                let line = Line(-display_offset + (screen_lines / 2) - 1);
                let col = first_occupied_in_line(term, line).unwrap_or_default().column;
                point = Point::new(line, col);
            },
            ViMotion::Low => {
                let display_offset = term.grid().display_offset() as i32;
                let screen_lines = term.screen_lines() as i32;

                let line = Line(-display_offset + screen_lines - 1);
                let col = first_occupied_in_line(term, line).unwrap_or_default().column;
                point = Point::new(line, col);
            },
            ViMotion::SemanticLeft => {
                point = semantic(term, point, Direction::Left, Side::Left);
            },
            ViMotion::SemanticRight => {
                point = semantic(term, point, Direction::Right, Side::Left);
            },
            ViMotion::SemanticLeftEnd => {
                point = semantic(term, point, Direction::Left, Side::Right);
            },
            ViMotion::SemanticRightEnd => {
                point = semantic(term, point, Direction::Right, Side::Right);
            },
            ViMotion::WordLeft => {
                point = word(term, point, Direction::Left, Side::Left);
            },
            ViMotion::WordRight => {
                point = word(term, point, Direction::Right, Side::Left);
            },
            ViMotion::WordLeftEnd => {
                point = word(term, point, Direction::Left, Side::Right);
            },
            ViMotion::WordRightEnd => {
                point = word(term, point, Direction::Right, Side::Right);
            },
            ViMotion::Bracket => point = term.bracket_search(point).unwrap_or(point),
        }
        point
    }

    /// Get target cursor point for vim-like page movement.
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn scroll<T: EventListener>(mut self, term: &Term<T>, lines: i32) -> Self {
        // Clamp movement to within visible region.
        let line = (self.point.line - lines).grid_clamp(term, Boundary::Grid);

        // Find the first occupied cell after scrolling has been performed.
        let column = first_occupied_in_line(term, line).unwrap_or_default().column;

        // Move cursor.
        self.point = Point::new(line, column);

        self
    }
}

/// Find next end of line to move to.
fn last<T>(term: &Term<T>, mut point: Point) -> Point {
    // Expand across wide cells.
    point = term.expand_wide(point, Direction::Right);

    // Find last non-empty cell in the current line.
    let occupied = last_occupied_in_line(term, point.line).unwrap_or_default();

    if point.column < occupied.column {
        // Jump to last occupied cell when not already at or beyond it.
        occupied
    } else if is_wrap(term, point) {
        // Jump to last occupied cell across linewraps.
        while is_wrap(term, point) {
            point.line += 1;
        }

        last_occupied_in_line(term, point.line).unwrap_or(point)
    } else {
        // Jump to last column when beyond the last occupied cell.
        Point::new(point.line, term.last_column())
    }
}

/// Find next non-empty cell to move to.
fn first_occupied<T>(term: &Term<T>, mut point: Point) -> Point {
    let last_column = term.last_column();

    // Expand left across wide chars, since we're searching lines left to right.
    point = term.expand_wide(point, Direction::Left);

    // Find first non-empty cell in current line.
    let occupied = first_occupied_in_line(term, point.line)
        .unwrap_or_else(|| Point::new(point.line, last_column));

    // Jump across wrapped lines if we're already at this line's first occupied cell.
    if point == occupied {
        let mut occupied = None;

        // Search for non-empty cell in previous lines.
        for line in (term.topmost_line().0..point.line.0).rev().map(Line::from) {
            if !is_wrap(term, Point::new(line, last_column)) {
                break;
            }

            occupied = first_occupied_in_line(term, line).or(occupied);
        }

        // Fallback to the next non-empty cell.
        let mut line = point.line;
        occupied.unwrap_or_else(|| loop {
            if let Some(occupied) = first_occupied_in_line(term, line) {
                break occupied;
            }

            let last_cell = Point::new(line, last_column);
            if !is_wrap(term, last_cell) {
                break last_cell;
            }

            line += 1;
        })
    } else {
        occupied
    }
}

/// Move by semantically separated word, like w/b/e/ge in vi.
fn semantic<T: EventListener>(
    term: &Term<T>,
    mut point: Point,
    direction: Direction,
    side: Side,
) -> Point {
    // Expand semantically based on movement direction.
    let expand_semantic = |point: Point| {
        // Do not expand when currently on a semantic escape char.
        let cell = &term.grid()[point];
        if term.semantic_escape_chars().contains(cell.c)
            && !cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            point
        } else if direction == Direction::Left {
            term.semantic_search_left(point)
        } else {
            term.semantic_search_right(point)
        }
    };

    // Make sure we jump above wide chars.
    point = term.expand_wide(point, direction);

    // Move to word boundary.
    if direction != side && !is_boundary(term, point, direction) {
        point = expand_semantic(point);
    }

    // Skip whitespace.
    let mut next_point = advance(term, point, direction);
    while !is_boundary(term, point, direction) && is_space(term, next_point) {
        point = next_point;
        next_point = advance(term, point, direction);
    }

    // Assure minimum movement of one cell.
    if !is_boundary(term, point, direction) {
        point = advance(term, point, direction);
    }

    // Move to word boundary.
    if direction == side && !is_boundary(term, point, direction) {
        point = expand_semantic(point);
    }

    point
}

/// Move by whitespace separated word, like W/B/E/gE in vi.
fn word<T: EventListener>(
    term: &Term<T>,
    mut point: Point,
    direction: Direction,
    side: Side,
) -> Point {
    // Make sure we jump above wide chars.
    point = term.expand_wide(point, direction);

    if direction == side {
        // Skip whitespace until right before a word.
        let mut next_point = advance(term, point, direction);
        while !is_boundary(term, point, direction) && is_space(term, next_point) {
            point = next_point;
            next_point = advance(term, point, direction);
        }

        // Skip non-whitespace until right inside word boundary.
        let mut next_point = advance(term, point, direction);
        while !is_boundary(term, point, direction) && !is_space(term, next_point) {
            point = next_point;
            next_point = advance(term, point, direction);
        }
    }

    if direction != side {
        // Skip non-whitespace until just beyond word.
        while !is_boundary(term, point, direction) && !is_space(term, point) {
            point = advance(term, point, direction);
        }

        // Skip whitespace until right inside word boundary.
        while !is_boundary(term, point, direction) && is_space(term, point) {
            point = advance(term, point, direction);
        }
    }

    point
}

/// Find first non-empty cell in line.
fn first_occupied_in_line<T>(term: &Term<T>, line: Line) -> Option<Point> {
    (0..term.columns())
        .map(|col| Point::new(line, Column(col)))
        .find(|&point| !is_space(term, point))
}

/// Find last non-empty cell in line.
fn last_occupied_in_line<T>(term: &Term<T>, line: Line) -> Option<Point> {
    (0..term.columns())
        .map(|col| Point::new(line, Column(col)))
        .rfind(|&point| !is_space(term, point))
}

/// Advance point based on direction.
fn advance<T>(term: &Term<T>, point: Point, direction: Direction) -> Point {
    if direction == Direction::Left {
        point.sub(term, Boundary::Grid, 1)
    } else {
        point.add(term, Boundary::Grid, 1)
    }
}

/// Check if cell at point contains whitespace.
fn is_space<T>(term: &Term<T>, point: Point) -> bool {
    let cell = &term.grid()[point.line][point.column];
    !cell.flags().intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        && (cell.c == ' ' || cell.c == '\t')
}

/// Check if the cell at a point contains the WRAPLINE flag.
fn is_wrap<T>(term: &Term<T>, point: Point) -> bool {
    term.grid()[point].flags.contains(Flags::WRAPLINE)
}

/// Check if point is at screen boundary.
fn is_boundary<T>(term: &Term<T>, point: Point, direction: Direction) -> bool {
    (point.line <= term.topmost_line() && point.column == 0 && direction == Direction::Left)
        || (point.line == term.bottommost_line()
            && point.column + 1 >= term.columns()
            && direction == Direction::Right)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::event::VoidListener;
    use crate::index::{Column, Line};
    use crate::term::test::TermSize;
    use crate::term::{Config, Term};
    use crate::vte::ansi::Handler;

    fn term() -> Term<VoidListener> {
        let size = TermSize::new(20, 20);
        Term::new(Config::default(), &size, VoidListener)
    }

    #[test]
    fn motion_simple() {
        let mut term = term();

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Right, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(1)));

        cursor = cursor.motion(&mut term, ViMotion::Left, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Down, 1);
        assert_eq!(cursor.point, Point::new(Line(1), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Up, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    #[test]
    fn simple_wide() {
        let mut term = term();
        term.grid_mut()[Line(0)][Column(0)].c = 'a';
        term.grid_mut()[Line(0)][Column(1)].c = '汉';
        term.grid_mut()[Line(0)][Column(1)].flags.insert(Flags::WIDE_CHAR);
        term.grid_mut()[Line(0)][Column(2)].c = ' ';
        term.grid_mut()[Line(0)][Column(2)].flags.insert(Flags::WIDE_CHAR_SPACER);
        term.grid_mut()[Line(0)][Column(3)].c = 'a';

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(1)));
        cursor = cursor.motion(&mut term, ViMotion::Right, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(3)));

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(2)));
        cursor = cursor.motion(&mut term, ViMotion::Left, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    #[test]
    fn motion_start_end() {
        let mut term = term();

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Last, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(19)));

        cursor = cursor.motion(&mut term, ViMotion::First, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    #[test]
    fn motion_first_occupied() {
        let mut term = term();
        term.grid_mut()[Line(0)][Column(0)].c = ' ';
        term.grid_mut()[Line(0)][Column(1)].c = 'x';
        term.grid_mut()[Line(0)][Column(2)].c = ' ';
        term.grid_mut()[Line(0)][Column(3)].c = 'y';
        term.grid_mut()[Line(0)][Column(19)].flags.insert(Flags::WRAPLINE);
        term.grid_mut()[Line(1)][Column(19)].flags.insert(Flags::WRAPLINE);
        term.grid_mut()[Line(2)][Column(0)].c = 'z';
        term.grid_mut()[Line(2)][Column(1)].c = ' ';

        let mut cursor = ViModeCursor::new(Point::new(Line(2), Column(1)));

        cursor = cursor.motion(&mut term, ViMotion::FirstOccupied, 1);
        assert_eq!(cursor.point, Point::new(Line(2), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::FirstOccupied, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(1)));
    }

    #[test]
    fn motion_high_middle_low() {
        let mut term = term();

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::High, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Middle, 1);
        assert_eq!(cursor.point, Point::new(Line(9), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Low, 1);
        assert_eq!(cursor.point, Point::new(Line(19), Column(0)));
    }

    #[test]
    fn motion_bracket() {
        let mut term = term();
        term.grid_mut()[Line(0)][Column(0)].c = '(';
        term.grid_mut()[Line(0)][Column(1)].c = 'x';
        term.grid_mut()[Line(0)][Column(2)].c = ')';

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Bracket, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(2)));

        cursor = cursor.motion(&mut term, ViMotion::Bracket, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    fn motion_semantic_term() -> Term<VoidListener> {
        let mut term = term();

        term.grid_mut()[Line(0)][Column(0)].c = 'x';
        term.grid_mut()[Line(0)][Column(1)].c = ' ';
        term.grid_mut()[Line(0)][Column(2)].c = 'x';
        term.grid_mut()[Line(0)][Column(3)].c = 'x';
        term.grid_mut()[Line(0)][Column(4)].c = ' ';
        term.grid_mut()[Line(0)][Column(5)].c = ' ';
        term.grid_mut()[Line(0)][Column(6)].c = ':';
        term.grid_mut()[Line(0)][Column(7)].c = ' ';
        term.grid_mut()[Line(0)][Column(8)].c = 'x';
        term.grid_mut()[Line(0)][Column(9)].c = ':';
        term.grid_mut()[Line(0)][Column(10)].c = 'x';
        term.grid_mut()[Line(0)][Column(11)].c = ' ';
        term.grid_mut()[Line(0)][Column(12)].c = ' ';
        term.grid_mut()[Line(0)][Column(13)].c = ':';
        term.grid_mut()[Line(0)][Column(14)].c = ' ';
        term.grid_mut()[Line(0)][Column(15)].c = 'x';

        term
    }

    #[test]
    fn motion_semantic_right_end() {
        let mut term = motion_semantic_term();

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(3)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(15)));
    }

    #[test]
    fn motion_semantic_left_start() {
        let mut term = motion_semantic_term();

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(15)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(2)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    #[test]
    fn motion_semantic_right_start() {
        let mut term = motion_semantic_term();

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(2)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(15)));
    }

    #[test]
    fn motion_semantic_left_end() {
        let mut term = motion_semantic_term();

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(15)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(3)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    #[test]
    fn scroll_semantic() {
        let mut term = term();
        term.grid_mut().scroll_up(&(Line(0)..Line(20)), 5);

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft, 1);
        assert_eq!(cursor.point, Point::new(Line(-5), Column(0)));
        assert_eq!(term.grid().display_offset(), 5);

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight, 1);
        assert_eq!(cursor.point, Point::new(Line(19), Column(19)));
        assert_eq!(term.grid().display_offset(), 0);

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(-5), Column(0)));
        assert_eq!(term.grid().display_offset(), 5);

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(19), Column(19)));
        assert_eq!(term.grid().display_offset(), 0);
    }

    #[test]
    fn semantic_wide() {
        let mut term = term();
        term.grid_mut()[Line(0)][Column(0)].c = 'a';
        term.grid_mut()[Line(0)][Column(1)].c = ' ';
        term.grid_mut()[Line(0)][Column(2)].c = '汉';
        term.grid_mut()[Line(0)][Column(2)].flags.insert(Flags::WIDE_CHAR);
        term.grid_mut()[Line(0)][Column(3)].c = ' ';
        term.grid_mut()[Line(0)][Column(3)].flags.insert(Flags::WIDE_CHAR_SPACER);
        term.grid_mut()[Line(0)][Column(4)].c = ' ';
        term.grid_mut()[Line(0)][Column(5)].c = 'a';

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(2)));
        cursor = cursor.motion(&mut term, ViMotion::SemanticRight, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(5)));

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(3)));
        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    #[test]
    fn motion_word() {
        let mut term = term();
        term.grid_mut()[Line(0)][Column(0)].c = 'a';
        term.grid_mut()[Line(0)][Column(1)].c = ';';
        term.grid_mut()[Line(0)][Column(2)].c = ' ';
        term.grid_mut()[Line(0)][Column(3)].c = ' ';
        term.grid_mut()[Line(0)][Column(4)].c = 'a';
        term.grid_mut()[Line(0)][Column(5)].c = ';';

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::WordRightEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(1)));

        cursor = cursor.motion(&mut term, ViMotion::WordRightEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(5)));

        cursor = cursor.motion(&mut term, ViMotion::WordLeft, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(4)));

        cursor = cursor.motion(&mut term, ViMotion::WordLeft, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::WordRight, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(4)));

        cursor = cursor.motion(&mut term, ViMotion::WordLeftEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(1)));
    }

    #[test]
    fn scroll_word() {
        let mut term = term();
        term.grid_mut().scroll_up(&(Line(0)..Line(20)), 5);

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::WordLeft, 1);
        assert_eq!(cursor.point, Point::new(Line(-5), Column(0)));
        assert_eq!(term.grid().display_offset(), 5);

        cursor = cursor.motion(&mut term, ViMotion::WordRight, 1);
        assert_eq!(cursor.point, Point::new(Line(19), Column(19)));
        assert_eq!(term.grid().display_offset(), 0);

        cursor = cursor.motion(&mut term, ViMotion::WordLeftEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(-5), Column(0)));
        assert_eq!(term.grid().display_offset(), 5);

        cursor = cursor.motion(&mut term, ViMotion::WordRightEnd, 1);
        assert_eq!(cursor.point, Point::new(Line(19), Column(19)));
        assert_eq!(term.grid().display_offset(), 0);
    }

    #[test]
    fn word_wide() {
        let mut term = term();
        term.grid_mut()[Line(0)][Column(0)].c = 'a';
        term.grid_mut()[Line(0)][Column(1)].c = ' ';
        term.grid_mut()[Line(0)][Column(2)].c = '汉';
        term.grid_mut()[Line(0)][Column(2)].flags.insert(Flags::WIDE_CHAR);
        term.grid_mut()[Line(0)][Column(3)].c = ' ';
        term.grid_mut()[Line(0)][Column(3)].flags.insert(Flags::WIDE_CHAR_SPACER);
        term.grid_mut()[Line(0)][Column(4)].c = ' ';
        term.grid_mut()[Line(0)][Column(5)].c = 'a';

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(2)));
        cursor = cursor.motion(&mut term, ViMotion::WordRight, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(5)));

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(3)));
        cursor = cursor.motion(&mut term, ViMotion::WordLeft, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    #[test]
    fn scroll_simple() {
        let mut term = term();

        // Create 1 line of scrollback.
        for _ in 0..20 {
            term.newline();
        }

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.scroll(&term, -1);
        assert_eq!(cursor.point, Point::new(Line(1), Column(0)));

        cursor = cursor.scroll(&term, 1);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));

        cursor = cursor.scroll(&term, 1);
        assert_eq!(cursor.point, Point::new(Line(-1), Column(0)));
    }

    #[test]
    fn scroll_over_top() {
        let mut term = term();

        // Create 40 lines of scrollback.
        for _ in 0..59 {
            term.newline();
        }

        let mut cursor = ViModeCursor::new(Point::new(Line(19), Column(0)));

        cursor = cursor.scroll(&term, 20);
        assert_eq!(cursor.point, Point::new(Line(-1), Column(0)));

        cursor = cursor.scroll(&term, 20);
        assert_eq!(cursor.point, Point::new(Line(-21), Column(0)));

        cursor = cursor.scroll(&term, 20);
        assert_eq!(cursor.point, Point::new(Line(-40), Column(0)));

        cursor = cursor.scroll(&term, 20);
        assert_eq!(cursor.point, Point::new(Line(-40), Column(0)));
    }

    #[test]
    fn scroll_over_bottom() {
        let mut term = term();

        // Create 40 lines of scrollback.
        for _ in 0..59 {
            term.newline();
        }

        let mut cursor = ViModeCursor::new(Point::new(Line(-40), Column(0)));

        cursor = cursor.scroll(&term, -20);
        assert_eq!(cursor.point, Point::new(Line(-20), Column(0)));

        cursor = cursor.scroll(&term, -20);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));

        cursor = cursor.scroll(&term, -20);
        assert_eq!(cursor.point, Point::new(Line(19), Column(0)));

        cursor = cursor.scroll(&term, -20);
        assert_eq!(cursor.point, Point::new(Line(19), Column(0)));
    }
}
