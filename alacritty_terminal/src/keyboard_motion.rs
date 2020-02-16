use std::cmp::min;

use crate::event::EventListener;
use crate::grid::{GridCell, Scroll};
use crate::index::{Column, Line, Point};
use crate::term::cell::Flags;
use crate::term::{Search, Term};

/// Possible keyboard motion movements.
pub enum KeyboardMotion {
    Up,
    Down,
    Left,
    Right,
    Start,
    End,
    High,
    Middle,
    Low,
    SemanticLeft,
    SemanticRight,
    SemanticLeftEnd,
    SemanticRightEnd,
    WordLeft,
    WordRight,
    WordLeftEnd,
    WordRightEnd,
    Bracket,
}

/// Cursor tracking keyboard motion position.
#[derive(Default, Copy, Clone)]
pub struct KeyboardCursor {
    pub point: Point,
}

impl KeyboardCursor {
    pub fn new(point: Point) -> Self {
        Self { point }
    }

    /// Move keyboard motion cursor.
    #[must_use = "this returns the result of the operation, without modifying the original"]
    pub fn motion<T: EventListener>(mut self, term: &mut Term<T>, motion: KeyboardMotion) -> Self {
        let lines = term.grid().num_lines();
        let cols = term.grid().num_cols();

        // Advance keyboard cursor
        match motion {
            KeyboardMotion::Up => {
                if self.point.line.0 == 0 {
                    term.scroll_display(Scroll::Lines(1));
                } else {
                    self.point.line -= 1;
                }
            },
            KeyboardMotion::Down => {
                if self.point.line >= lines - 1 {
                    term.scroll_display(Scroll::Lines(-1));
                } else {
                    self.point.line += 1;
                }
            },
            KeyboardMotion::Left => {
                self.point = expand_wide(term, self.point, true);
                self.point.col = Column(self.point.col.saturating_sub(1));
            }
            KeyboardMotion::Right => {
                self.point = expand_wide(term, self.point, false);
                self.point.col = min(self.point.col + 1, cols - 1);
            }
            KeyboardMotion::Start => self.point.col = Column(0),
            KeyboardMotion::End => self.point.col = cols - 1,
            KeyboardMotion::High => self.point = Point::new(Line(0), Column(0)),
            KeyboardMotion::Middle => self.point = Point::new(Line((lines.0 - 1) / 2), Column(0)),
            KeyboardMotion::Low => self.point = Point::new(lines - 1, Column(0)),
            KeyboardMotion::SemanticLeft => {
                self.point = Self::semantic_move(term, self.point, true, true)
            },
            KeyboardMotion::SemanticRight => {
                self.point = Self::semantic_move(term, self.point, false, true)
            },
            KeyboardMotion::SemanticLeftEnd => {
                self.point = Self::semantic_move(term, self.point, true, false)
            },
            KeyboardMotion::SemanticRightEnd => {
                self.point = Self::semantic_move(term, self.point, false, false)
            },
            KeyboardMotion::WordLeft => self.point = Self::word_move(term, self.point, true, true),
            KeyboardMotion::WordRight => {
                self.point = Self::word_move(term, self.point, false, true)
            },
            KeyboardMotion::WordLeftEnd => {
                self.point = Self::word_move(term, self.point, true, false)
            },
            KeyboardMotion::WordRightEnd => {
                self.point = Self::word_move(term, self.point, false, false)
            },
            KeyboardMotion::Bracket => {
                if let Some(p) = term.bracket_search(term.visible_to_buffer(self.point)) {
                    // Scroll viewport if necessary
                    scroll(term, p);

                    self.point = term.buffer_to_visible(p).unwrap_or_else(Point::default).into();
                }
            },
        }

        self
    }

    /// Move by semanticuation separated word, like w/b/e/ge in vi.
    fn semantic_move<T: EventListener>(
        term: &mut Term<T>,
        point: Point,
        left: bool,
        start: bool,
    ) -> Point {
        // Expand semantically based on movement direction
        let semantic = |point: Point<usize>| {
            // Do not expand when currently on an escape char
            let cell = term.grid()[point.line][point.col];
            if term.semantic_escape_chars().contains(cell.c)
                && !cell.flags.contains(Flags::WIDE_CHAR_SPACER)
            {
                point
            } else if left {
                term.semantic_search_left(point)
            } else {
                term.semantic_search_right(point)
            }
        };

        // Make sure we jump above wide chars
        let point = expand_wide(term, point, left);

        let mut buffer_point = term.visible_to_buffer(point);

        // Move to word boundary
        if !is_boundary(term, buffer_point, left) && left != start {
            buffer_point = semantic(buffer_point);
        }

        // Skip whitespace
        let mut point = advance(term, buffer_point, left);
        while !is_boundary(term, buffer_point, left) && is_space(term, point) {
            buffer_point = point;
            point = advance(term, buffer_point, left);
        }

        // Assure minimum movement of one cell
        if !is_boundary(term, buffer_point, left) {
            buffer_point = advance(term, buffer_point, left);
        }

        // Move to word boundary
        if !is_boundary(term, buffer_point, left) && left == start {
            buffer_point = semantic(buffer_point);
        }

        // Scroll viewport if necessary
        scroll(term, buffer_point);

        term.buffer_to_visible(buffer_point).unwrap_or_else(Point::default).into()
    }

    /// Move by whitespace separated word, like W/B/E/gE in vi.
    fn word_move<T: EventListener>(
        term: &mut Term<T>,
        point: Point,
        left: bool,
        start: bool,
    ) -> Point {
        // Make sure we jump above wide chars
        let point = expand_wide(term, point, left);

        let mut buffer_point = term.visible_to_buffer(point);

        // Skip whitespace until right before a word
        if left == start {
            let mut point = advance(term, buffer_point, left);
            while !is_boundary(term, buffer_point, left) && is_space(term, point) {
                buffer_point = point;
                point = advance(term, point, left);
            }
        }

        if left == start {
            // Skip non-whitespace until right inside word boundary
            let mut point = advance(term, buffer_point, left);
            while !is_boundary(term, buffer_point, left) && !is_space(term, point) {
                buffer_point = point;
                point = advance(term, buffer_point, left);
            }
        } else {
            // Skip non-whitespace until just beyond word
            while !is_boundary(term, buffer_point, left) && !is_space(term, buffer_point) {
                buffer_point = advance(term, buffer_point, left);
            }
        };

        // Skip whitespace until right inside word boundary
        if left != start {
            while !is_boundary(term, buffer_point, left) && is_space(term, buffer_point) {
                buffer_point = advance(term, buffer_point, left);
            }
        }

        // Scroll viewport if necessary
        scroll(term, buffer_point);

        term.buffer_to_visible(buffer_point).unwrap_or_else(Point::default).into()
    }
}

/// Scroll display if point is outside of viewport.
fn scroll<T: EventListener>(term: &mut Term<T>, point: Point<usize>) {
    let display_offset = term.grid().display_offset();
    let lines = term.grid().num_lines();

    // Scroll once the top/bottom has been reached
    if point.line >= display_offset + lines.0 {
        let lines = point.line.saturating_sub(display_offset + lines.0 - 1);
        term.scroll_display(Scroll::Lines(lines as isize));
    } else if point.line < display_offset {
        let lines = display_offset.saturating_sub(point.line);
        term.scroll_display(Scroll::Lines(-(lines as isize)));
    };
}

/// Jump to the end of a wide cell.
fn expand_wide<T>(term: &Term<T>, mut point: Point, left: bool) -> Point {
    let cell = term.grid()[point.line][point.col];

    if cell.flags.contains(Flags::WIDE_CHAR) && !left {
        point.col += 1;
    } else if cell.flags.contains(Flags::WIDE_CHAR_SPACER)
        && term.grid()[point.line][point.col - 1].flags.contains(Flags::WIDE_CHAR)
        && left
    {
        point.col -= 1;
    }

    point
}

/// Check if cell at point contains whitespace.
fn is_space<T>(term: &Term<T>, point: Point<usize>) -> bool {
    let cell = term.grid()[point.line][point.col];
    cell.c == ' ' && !cell.flags().contains(Flags::WIDE_CHAR_SPACER)
}

/// Check if point is at screen boundary.
fn is_boundary<T>(term: &Term<T>, point: Point<usize>, left: bool) -> bool {
    (point.line == 0 && point.col + 1 >= term.grid().num_cols() && !left)
        || (point.line + 1 >= term.grid().len() && point.col.0 == 0 && left)
}

/// Advance point based on direction.
fn advance<T>(term: &Term<T>, point: Point<usize>, left: bool) -> Point<usize> {
    let cols = term.grid().num_cols();
    if left {
        point.sub_absolute(cols.0, 1)
    } else {
        point.add_absolute(cols.0, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::clipboard::Clipboard;
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
        Term::new(&MockConfig::default(), &size, Clipboard::new_nop(), Mock)
    }

    #[test]
    fn motion_simple() {
        let mut term = term();

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, KeyboardMotion::Right);
        assert_eq!(cursor.point, Point::new(Line(0), Column(1)));

        cursor = cursor.motion(&mut term, KeyboardMotion::Left);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, KeyboardMotion::Down);
        assert_eq!(cursor.point, Point::new(Line(1), Column(0)));

        cursor = cursor.motion(&mut term, KeyboardMotion::Up);
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

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(1)));
        cursor = cursor.motion(&mut term, KeyboardMotion::Right);
        assert_eq!(cursor.point, Point::new(Line(0), Column(3)));

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(2)));
        cursor = cursor.motion(&mut term, KeyboardMotion::Left);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    #[test]
    fn motion_start_end() {
        let mut term = term();

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, KeyboardMotion::End);
        assert_eq!(cursor.point, Point::new(Line(0), Column(19)));

        cursor = cursor.motion(&mut term, KeyboardMotion::Start);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    #[test]
    fn motion_high_middle_low() {
        let mut term = term();

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, KeyboardMotion::High);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, KeyboardMotion::Middle);
        assert_eq!(cursor.point, Point::new(Line(9), Column(0)));

        cursor = cursor.motion(&mut term, KeyboardMotion::Low);
        assert_eq!(cursor.point, Point::new(Line(19), Column(0)));
    }

    #[test]
    fn motion_bracket() {
        let mut term = term();
        term.grid_mut()[Line(0)][Column(0)].c = '(';
        term.grid_mut()[Line(0)][Column(1)].c = 'x';
        term.grid_mut()[Line(0)][Column(2)].c = ')';

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, KeyboardMotion::Bracket);
        assert_eq!(cursor.point, Point::new(Line(0), Column(2)));

        cursor = cursor.motion(&mut term, KeyboardMotion::Bracket);
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

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(3)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(15)));
    }

    #[test]
    fn motion_semantic_left_start() {
        let mut term = motion_semantic_term();

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(15)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(2)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    #[test]
    fn motion_semantic_right_start() {
        let mut term = motion_semantic_term();

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(2)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(15)));
    }

    #[test]
    fn motion_semantic_left_end() {
        let mut term = motion_semantic_term();

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(15)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(13)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(10)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(9)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(8)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(6)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(3)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }

    #[test]
    fn scroll_semantic() {
        let mut term = term();
        term.grid_mut().scroll_up(&(Line(0)..Line(20)), Line(5), &Default::default());

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
        assert_eq!(term.grid().display_offset(), 5);

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(19), Column(19)));
        assert_eq!(term.grid().display_offset(), 0);

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
        assert_eq!(term.grid().display_offset(), 5);

        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRightEnd);
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

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(2)));
        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(5)));

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(3)));
        cursor = cursor.motion(&mut term, KeyboardMotion::SemanticLeft);
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

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, KeyboardMotion::WordRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(1)));

        cursor = cursor.motion(&mut term, KeyboardMotion::WordRightEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(5)));

        cursor = cursor.motion(&mut term, KeyboardMotion::WordLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(4)));

        cursor = cursor.motion(&mut term, KeyboardMotion::WordLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, KeyboardMotion::WordRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(4)));

        cursor = cursor.motion(&mut term, KeyboardMotion::WordLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(1)));
    }

    #[test]
    fn scroll_word() {
        let mut term = term();
        term.grid_mut().scroll_up(&(Line(0)..Line(20)), Line(5), &Default::default());

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(0)));

        cursor = cursor.motion(&mut term, KeyboardMotion::WordLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
        assert_eq!(term.grid().display_offset(), 5);

        cursor = cursor.motion(&mut term, KeyboardMotion::WordRight);
        assert_eq!(cursor.point, Point::new(Line(19), Column(19)));
        assert_eq!(term.grid().display_offset(), 0);

        cursor = cursor.motion(&mut term, KeyboardMotion::WordLeftEnd);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
        assert_eq!(term.grid().display_offset(), 5);

        cursor = cursor.motion(&mut term, KeyboardMotion::WordRightEnd);
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

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(2)));
        cursor = cursor.motion(&mut term, KeyboardMotion::WordRight);
        assert_eq!(cursor.point, Point::new(Line(0), Column(5)));

        let mut cursor = KeyboardCursor::new(Point::new(Line(0), Column(3)));
        cursor = cursor.motion(&mut term, KeyboardMotion::WordLeft);
        assert_eq!(cursor.point, Point::new(Line(0), Column(0)));
    }
}
