use std::cmp::{max, min};

use serde::Deserialize;

use crate::event::EventListener;
use crate::grid::{Dimensions, GridCell};
use crate::index::{Boundary, Column, Direction, Line, Point, Side};
use crate::term::cell::Flags;
use crate::term::Term;

/// Possible vi mode motion movements.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize)]
pub enum ViMotion {
    /// Move up.
    Up,
    /// Move down.
    Down,
    /// Move left.
    Left,
    /// Move right.
    Right,
    /// Move to start of line.
    First,
    /// Move to end of line.
    Last,
    /// Move to the first non-empty cell.
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
#[derive(Default, Copy, Clone)]
pub struct ViModeCursor {
    pub point: Point,
}

impl ViModeCursor {
    pub fn new(point: Point) -> Self {
        Self { point }
    }

    /// Move vi mode cursor.
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn motion<T: EventListener>(mut self, term: &mut Term<T>, motion: ViMotion) -> Self {
        let display_offset = term.grid().display_offset();
        let lines = term.grid().screen_lines();
        let cols = term.grid().cols();

        let mut buffer_point = term.visible_to_buffer(self.point);

        match motion {
            ViMotion::Up => {
                if buffer_point.line + 1 < term.grid().total_lines() {
                    buffer_point.line += 1;
                }
            },
            ViMotion::Down => buffer_point.line = buffer_point.line.saturating_sub(1),
            ViMotion::Left => {
                buffer_point = term.expand_wide(buffer_point, Direction::Left);
                let wrap_point = Point::new(buffer_point.line + 1, cols - 1);
                if buffer_point.col.0 == 0
                    && buffer_point.line + 1 < term.grid().total_lines()
                    && is_wrap(term, wrap_point)
                {
                    buffer_point = wrap_point;
                } else {
                    buffer_point.col = Column(buffer_point.col.saturating_sub(1));
                }
            },
            ViMotion::Right => {
                buffer_point = term.expand_wide(buffer_point, Direction::Right);
                if is_wrap(term, buffer_point) {
                    buffer_point = Point::new(buffer_point.line - 1, Column(0));
                } else {
                    buffer_point.col = min(buffer_point.col + 1, cols - 1);
                }
            },
            ViMotion::First => {
                buffer_point = term.expand_wide(buffer_point, Direction::Left);
                while buffer_point.col.0 == 0
                    && buffer_point.line + 1 < term.grid().total_lines()
                    && is_wrap(term, Point::new(buffer_point.line + 1, cols - 1))
                {
                    buffer_point.line += 1;
                }
                buffer_point.col = Column(0);
            },
            ViMotion::Last => buffer_point = last(term, buffer_point),
            ViMotion::FirstOccupied => buffer_point = first_occupied(term, buffer_point),
            ViMotion::High => {
                let line = display_offset + lines.0 - 1;
                let col = first_occupied_in_line(term, line).unwrap_or_default().col;
                buffer_point = Point::new(line, col);
            },
            ViMotion::Middle => {
                let line = display_offset + lines.0 / 2;
                let col = first_occupied_in_line(term, line).unwrap_or_default().col;
                buffer_point = Point::new(line, col);
            },
            ViMotion::Low => {
                let line = display_offset;
                let col = first_occupied_in_line(term, line).unwrap_or_default().col;
                buffer_point = Point::new(line, col);
            },
            ViMotion::SemanticLeft => {
                buffer_point = semantic(term, buffer_point, Direction::Left, Side::Left);
            },
            ViMotion::SemanticRight => {
                buffer_point = semantic(term, buffer_point, Direction::Right, Side::Left);
            },
            ViMotion::SemanticLeftEnd => {
                buffer_point = semantic(term, buffer_point, Direction::Left, Side::Right);
            },
            ViMotion::SemanticRightEnd => {
                buffer_point = semantic(term, buffer_point, Direction::Right, Side::Right);
            },
            ViMotion::WordLeft => {
                buffer_point = word(term, buffer_point, Direction::Left, Side::Left);
            },
            ViMotion::WordRight => {
                buffer_point = word(term, buffer_point, Direction::Right, Side::Left);
            },
            ViMotion::WordLeftEnd => {
                buffer_point = word(term, buffer_point, Direction::Left, Side::Right);
            },
            ViMotion::WordRightEnd => {
                buffer_point = word(term, buffer_point, Direction::Right, Side::Right);
            },
            ViMotion::Bracket => {
                buffer_point = term.bracket_search(buffer_point).unwrap_or(buffer_point);
            },
        }

        term.scroll_to_point(buffer_point);
        self.point = term.grid().clamp_buffer_to_visible(buffer_point);

        self
    }

    /// Get target cursor point for vim-like page movement.
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn scroll<T: EventListener>(mut self, term: &Term<T>, lines: isize) -> Self {
        // Check number of lines the cursor needs to be moved.
        let overscroll = if lines > 0 {
            let max_scroll = term.grid().history_size() - term.grid().display_offset();
            max(0, lines - max_scroll as isize)
        } else {
            let max_scroll = term.grid().display_offset();
            min(0, lines + max_scroll as isize)
        };

        // Clamp movement to within visible region.
        let mut line = self.point.line.0 as isize;
        line -= overscroll;
        line = max(0, min(term.grid().screen_lines().0 as isize - 1, line));

        // Find the first occupied cell after scrolling has been performed.
        let buffer_point = term.visible_to_buffer(self.point);
        let mut target_line = buffer_point.line as isize + lines;
        target_line = max(0, min(term.grid().total_lines() as isize - 1, target_line));
        let col = first_occupied_in_line(term, target_line as usize).unwrap_or_default().col;

        // Move cursor.
        self.point = Point::new(Line(line as usize), col);

        self
    }
}

/// Find next end of line to move to.
fn last<T>(term: &Term<T>, mut point: Point<usize>) -> Point<usize> {
    let cols = term.grid().cols();

    // Expand across wide cells.
    point = term.expand_wide(point, Direction::Right);

    // Find last non-empty cell in the current line.
    let occupied = last_occupied_in_line(term, point.line).unwrap_or_default();

    if point.col < occupied.col {
        // Jump to last occupied cell when not already at or beyond it.
        occupied
    } else if is_wrap(term, point) {
        // Jump to last occupied cell across linewraps.
        while point.line > 0 && is_wrap(term, point) {
            point.line -= 1;
        }

        last_occupied_in_line(term, point.line).unwrap_or(point)
    } else {
        // Jump to last column when beyond the last occupied cell.
        Point::new(point.line, cols - 1)
    }
}

/// Find next non-empty cell to move to.
fn first_occupied<T>(term: &Term<T>, mut point: Point<usize>) -> Point<usize> {
    let cols = term.grid().cols();

    // Expand left across wide chars, since we're searching lines left to right.
    point = term.expand_wide(point, Direction::Left);

    // Find first non-empty cell in current line.
    let occupied = first_occupied_in_line(term, point.line)
        .unwrap_or_else(|| Point::new(point.line, cols - 1));

    // Jump across wrapped lines if we're already at this line's first occupied cell.
    if point == occupied {
        let mut occupied = None;

        // Search for non-empty cell in previous lines.
        for line in (point.line + 1)..term.grid().total_lines() {
            if !is_wrap(term, Point::new(line, cols - 1)) {
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

            let last_cell = Point::new(line, cols - 1);
            if line == 0 || !is_wrap(term, last_cell) {
                break last_cell;
            }

            line -= 1;
        })
    } else {
        occupied
    }
}

/// Move by semantically separated word, like w/b/e/ge in vi.
fn semantic<T: EventListener>(
    term: &mut Term<T>,
    mut point: Point<usize>,
    direction: Direction,
    side: Side,
) -> Point<usize> {
    // Expand semantically based on movement direction.
    let expand_semantic = |point: Point<usize>| {
        // Do not expand when currently on a semantic escape char.
        let cell = term.grid()[point.line][point.col];
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
    term: &mut Term<T>,
    mut point: Point<usize>,
    direction: Direction,
    side: Side,
) -> Point<usize> {
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
fn first_occupied_in_line<T>(term: &Term<T>, line: usize) -> Option<Point<usize>> {
    (0..term.grid().cols().0)
        .map(|col| Point::new(line, Column(col)))
        .find(|&point| !is_space(term, point))
}

/// Find last non-empty cell in line.
fn last_occupied_in_line<T>(term: &Term<T>, line: usize) -> Option<Point<usize>> {
    (0..term.grid().cols().0)
        .map(|col| Point::new(line, Column(col)))
        .rfind(|&point| !is_space(term, point))
}

/// Advance point based on direction.
fn advance<T>(term: &Term<T>, point: Point<usize>, direction: Direction) -> Point<usize> {
    if direction == Direction::Left {
        point.sub_absolute(term, Boundary::Clamp, 1)
    } else {
        point.add_absolute(term, Boundary::Clamp, 1)
    }
}

/// Check if cell at point contains whitespace.
fn is_space<T>(term: &Term<T>, point: Point<usize>) -> bool {
    let cell = term.grid()[point.line][point.col];
    !cell.flags().intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        && (cell.c == ' ' || cell.c == '\t')
}

fn is_wrap<T>(term: &Term<T>, point: Point<usize>) -> bool {
    point.line != 0 && term.grid()[point.line][point.col].flags.contains(Flags::WRAPLINE)
}

/// Check if point is at screen boundary.
fn is_boundary<T>(term: &Term<T>, point: Point<usize>, direction: Direction) -> bool {
    let total_lines = term.grid().total_lines();
    let num_cols = term.grid().cols();
    (point.line + 1 >= total_lines && point.col.0 == 0 && direction == Direction::Left)
        || (point.line == 0 && point.col + 1 >= num_cols && direction == Direction::Right)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::MockConfig;
    use crate::event::Event;
    use crate::index::{Column, Line};
    use crate::term::{SizeInfo, Term};

    struct Mock;
    impl EventListener for Mock {
        fn send_event(&self, _event: Event) {}
    }

    fn term() -> Term<Mock> {
        let size = SizeInfo {
            width: 20.,
            height: 20.,
            cell_width: 1.0,
            cell_height: 1.0,
            padding_x: 0.0,
            padding_y: 0.0,
            dpr: 1.0,
        };
        Term::new(&MockConfig::default(), &size, Mock)
    }

    #[test]
    fn motion_simple() {
        let mut term = term();

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Right);
        assert_eq!(cursor.point, Point::new(Line(0), Column(1)));

        cursor = cursor.motion(&mut term, ViMotion::Left);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Down);
        assert_eq!(cursor.point, Point::new(Line(1), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Up);
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
        cursor = cursor.motion(&mut term, ViMotion::Right);
        assert_eq!(cursor.point, Point::new(Line(0), Column(3)));

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(2)));
        cursor = cursor.motion(&mut term, ViMotion::Left);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    #[test]
    fn motion_start_end() {
        let mut term = term();

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Last);
        assert_eq!(cursor.point, Point::new(Line(0), Column(19)));

        cursor = cursor.motion(&mut term, ViMotion::First);
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

        cursor = cursor.motion(&mut term, ViMotion::FirstOccupied);
        assert_eq!(cursor.point, Point::new(Line(2), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::FirstOccupied);
        assert_eq!(cursor.point, Point::new(Line(0), Column(1)));
    }

    #[test]
    fn motion_high_middle_low() {
        let mut term = term();

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::High);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Middle);
        assert_eq!(cursor.point, Point::new(Line(9), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Low);
        assert_eq!(cursor.point, Point::new(Line(19), Column(0)));
    }

    #[test]
    fn motion_bracket() {
        let mut term = term();
        term.grid_mut()[Line(0)][Column(0)].c = '(';
        term.grid_mut()[Line(0)][Column(1)].c = 'x';
        term.grid_mut()[Line(0)][Column(2)].c = ')';

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::Bracket);
        assert_eq!(cursor.point, Point::new(Line(0), Column(2)));

        cursor = cursor.motion(&mut term, ViMotion::Bracket);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    fn motion_semantic_term() -> Term<Mock> {
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

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(3)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(15)));
    }

    #[test]
    fn motion_semantic_left_start() {
        let mut term = motion_semantic_term();

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(15)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(2)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    #[test]
    fn motion_semantic_right_start() {
        let mut term = motion_semantic_term();

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(2)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(15)));
    }

    #[test]
    fn motion_semantic_left_end() {
        let mut term = motion_semantic_term();

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(15)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(3)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    #[test]
    fn scroll_semantic() {
        let mut term = term();
        term.grid_mut().scroll_up(&(Line(0)..Line(20)), Line(5), Default::default());

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
        assert_eq!(term.grid().display_offset(), 5);

        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(19), Column(19)));
        assert_eq!(term.grid().display_offset(), 0);

        cursor = cursor.motion(&mut term, ViMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
        assert_eq!(term.grid().display_offset(), 5);

        cursor = cursor.motion(&mut term, ViMotion::SemanticRightEnd);
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
        cursor = cursor.motion(&mut term, ViMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(5)));

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(3)));
        cursor = cursor.motion(&mut term, ViMotion::SemanticLeft);
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

        cursor = cursor.motion(&mut term, ViMotion::WordRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(1)));

        cursor = cursor.motion(&mut term, ViMotion::WordRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(5)));

        cursor = cursor.motion(&mut term, ViMotion::WordLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(4)));

        cursor = cursor.motion(&mut term, ViMotion::WordLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::WordRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(4)));

        cursor = cursor.motion(&mut term, ViMotion::WordLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(1)));
    }

    #[test]
    fn scroll_word() {
        let mut term = term();
        term.grid_mut().scroll_up(&(Line(0)..Line(20)), Line(5), Default::default());

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, ViMotion::WordLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
        assert_eq!(term.grid().display_offset(), 5);

        cursor = cursor.motion(&mut term, ViMotion::WordRight);
        assert_eq!(cursor.point, Point::new(Line(19), Column(19)));
        assert_eq!(term.grid().display_offset(), 0);

        cursor = cursor.motion(&mut term, ViMotion::WordLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
        assert_eq!(term.grid().display_offset(), 5);

        cursor = cursor.motion(&mut term, ViMotion::WordRightEnd);
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
        cursor = cursor.motion(&mut term, ViMotion::WordRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(5)));

        let mut cursor = ViModeCursor::new(Point::new(Line(0), Column(3)));
        cursor = cursor.motion(&mut term, ViMotion::WordLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }
}
