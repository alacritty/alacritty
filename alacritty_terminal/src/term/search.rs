use std::cmp::min;
use std::mem;
use std::ops::RangeInclusive;

use regex_automata::{Regex, DFA, Error as RegexError};

use crate::grid::{BidirectionalIterator, GridIterator};
use crate::index::{Column, Point};
use crate::term::cell::{Cell, Flags};
use crate::term::Term;

/// Used to match equal brackets, when performing a bracket-pair selection.
const BRACKET_PAIRS: [(char, char); 4] = [('(', ')'), ('[', ']'), ('{', '}'), ('<', '>')];

#[derive(Debug)]
pub struct RegexSearch {
    pattern: Regex,
    current_match: Option<RangeInclusive<Point<usize>>>,
}

impl RegexSearch {
    pub fn new(regex: &str) -> Result<Self, RegexError> {
        Ok(RegexSearch {
            pattern: Regex::new(regex)?,
            current_match: None,
        })
    }

    pub fn next<T>(&mut self, term: &Term<T>) {
        let start_point = if let Some(current_match) = &self.current_match {
            // TODO: Should we use start(), or end()? Vim doesn't allow nested matches.
            // TODO: Should use keyboard cursor instead of previous match
            current_match.end().add_absolute(term.grid().num_cols().0, 1)
        } else {
            Point::new(term.grid().num_lines().0 - 1, Column(0))
        };

        if let Some(regex_match) = term.regex_search_right(start_point, &self.pattern) {
            self.current_match = Some(regex_match);
        }
    }

    pub fn previous<T>(&mut self, term: &Term<T>) {
        let start_point = if let Some(current_match) = &self.current_match {
            // TODO: Should use keyboard cursor instead of previous match
            current_match.start().sub_absolute(term.grid().num_cols().0, 1)
        } else {
            Point::new(term.grid().num_lines().0 - 1, Column(0))
        };

        if let Some(regex_match) = term.regex_search_left(start_point, &self.pattern) {
            self.current_match = Some(regex_match);
        }
    }

    pub fn current_match(&self) -> Option<&RangeInclusive<Point<usize>>> {
        self.current_match.as_ref()
    }
}

impl<T> Term<T> {
    pub fn semantic_search_left(&self, mut point: Point<usize>) -> Point<usize> {
        // Limit the starting point to the last line in the history
        point.line = min(point.line, self.grid.len() - 1);

        let mut iter = self.grid.iter_from(point);
        let last_col = self.grid.num_cols() - Column(1);

        while let Some(cell) = iter.prev() {
            if !cell.flags.intersects(Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER)
                && self.semantic_escape_chars.contains(cell.c)
            {
                break;
            }

            if iter.point().col == last_col && !cell.flags.contains(Flags::WRAPLINE) {
                break; // cut off if on new line or hit escape char
            }

            point = iter.point();
        }

        point
    }

    pub fn semantic_search_right(&self, mut point: Point<usize>) -> Point<usize> {
        // Limit the starting point to the last line in the history
        point.line = min(point.line, self.grid.len() - 1);

        let mut iter = self.grid.iter_from(point);
        let last_col = self.grid.num_cols() - 1;

        while let Some(cell) = iter.next() {
            if !cell.flags.intersects(Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER)
                && self.semantic_escape_chars.contains(cell.c)
            {
                break;
            }

            point = iter.point();

            if point.col == last_col && !cell.flags.contains(Flags::WRAPLINE) {
                break; // cut off if on new line or hit escape char
            }
        }

        point
    }

    pub fn line_search_left(&self, mut point: Point<usize>) -> Point<usize> {
        while point.line + 1 < self.grid.len()
            && self.grid[point.line + 1][self.grid.num_cols() - 1].flags.contains(Flags::WRAPLINE)
        {
            point.line += 1;
        }

        point.col = Column(0);

        point
    }

    pub fn line_search_right(&self, mut point: Point<usize>) -> Point<usize> {
        while self.grid[point.line][self.grid.num_cols() - 1].flags.contains(Flags::WRAPLINE) {
            point.line -= 1;
        }

        point.col = self.grid.num_cols() - 1;

        point
    }

    /// Find the next regex match to the left of the origin point.
    ///
    /// The origin is always included in the regex.
    pub fn regex_search_left(
        &self,
        point: Point<usize>,
        regex: &Regex,
    ) -> Option<RangeInclusive<Point<usize>>> {
        let rdfa = regex.reverse();
        let fdfa = regex.forward();

        let mut iter = self.grid().iter_from(point);

        if let Some(start) = self.regex_search(&mut iter, |x| x.prev(), &rdfa) {
            iter.next();

            if let Some(end) = self.regex_search(&mut iter, |x| x.next(), &fdfa) {
                return Some(start..=end);
            }
        }

        None
    }

    /// Find the next regex match to the right of the origin point.
    ///
    /// The origin is always included in the regex.
    pub fn regex_search_right(
        &self,
        point: Point<usize>,
        regex: &Regex,
    ) -> Option<RangeInclusive<Point<usize>>> {
        let rdfa = regex.reverse();
        let fdfa = regex.forward();

        let mut iter = self.grid().iter_from(point);

        if let Some(end) = self.regex_search(&mut iter, |x| x.next(), &fdfa) {
            iter.prev();

            if let Some(start) = self.regex_search(&mut iter, |x| x.prev(), &rdfa) {
                return Some(start..=end);
            }
        }

        None
    }

    /// Find the next regex match.
    ///
    /// This will always return the side of the first match which is farthest from the start point.
    fn regex_search<F>(
        &self,
        iter: &mut GridIterator<'_, Cell>,
        mut iter_fn: F,
        dfa: &impl DFA,
    ) -> Option<Point<usize>>
    where
        F: for<'a, 'r> FnMut(&'a mut GridIterator<'r, Cell>) -> Option<&'a Cell>,
    {
        let last_col = self.grid().num_cols() - 1;

        let mut state = dfa.start_state();
        let mut point = iter.point();
        let mut cell = *iter.cell();
        let mut regex_match = None;

        loop {
            // Advance regex parser by one `char`
            let mut buf = [0; 4];
            cell.c.encode_utf8(&mut buf);
            for b in 0..cell.c.len_utf8() {
                // Since we get the state from the DFA, it doesn't need to be checked
                state = unsafe { dfa.next_state_unchecked(state, buf[b]) };
            }

            // Handle regex state changes
            if dfa.is_match_or_dead_state(state) {
                if dfa.is_dead_state(state) {
                    if regex_match.is_some() {
                        return regex_match;
                    }

                    state = dfa.start_state();
                } else {
                    regex_match = Some(point);
                }
            }

            // Advance iterator
            let new_cell = match iter_fn(iter) {
                Some(&cell) => cell,
                None => break,
            };
            let last_point = mem::replace(&mut point, iter.point());
            let last_cell = mem::replace(&mut cell, new_cell);

            // Reset on line breaks
            if (last_point.col == last_col && !last_cell.flags.contains(Flags::WRAPLINE))
                || (last_point.col == Column(0)
                    && point.col == last_col
                    && !cell.flags.contains(Flags::WRAPLINE))
            {
                if regex_match.is_some() {
                    return regex_match;
                }

                state = dfa.start_state();
            }
        }

        regex_match
    }

    pub fn bracket_search(&self, point: Point<usize>) -> Option<Point<usize>> {
        let start_char = self.grid[point.line][point.col].c;

        // Find the matching bracket we're looking for
        let (forwards, end_char) = BRACKET_PAIRS.iter().find_map(|(open, close)| {
            if open == &start_char {
                Some((true, *close))
            } else if close == &start_char {
                Some((false, *open))
            } else {
                None
            }
        })?;

        let mut iter = self.grid.iter_from(point);

        // For every character match that equals the starting bracket, we
        // ignore one bracket of the opposite type.
        let mut skip_pairs = 0;

        loop {
            // Check the next cell
            let cell = if forwards { iter.next() } else { iter.prev() };

            // Break if there are no more cells
            let c = match cell {
                Some(cell) => cell.c,
                None => break,
            };

            // Check if the bracket matches
            if c == end_char && skip_pairs == 0 {
                return Some(iter.point());
            } else if c == start_char {
                skip_pairs += 1;
            } else if c == end_char {
                skip_pairs -= 1;
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::index::{Column, Line};
    use crate::term::test::mock_term;

    #[test]
    fn no_url() {
        #[rustfmt::skip]
        let term = mock_term("\
            testing a long thing without URL\n\
            huh https://example.org 134\n\
            test short\
        ");

        assert_eq!(term.url_at_point(Point::new(Line(2), Column(22))), None);
        assert_eq!(term.url_at_point(Point::new(Line(1), Column(3))), None);
        assert_eq!(term.url_at_point(Point::new(Line(1), Column(23))), None);
        assert_eq!(term.url_at_point(Point::new(Line(0), Column(4))), None);
    }

    #[test]
    fn urls() {
        #[rustfmt::skip]
        let term = mock_term("\
            testing\n\
            huh https://example.org/1 https://example.org/2 134\n\
            test\
        ");

        let start = Point::new(1, Column(4));
        let end = Point::new(1, Column(24));
        assert_eq!(term.url_at_point(start.into()), Some(start..=end));
        assert_eq!(term.url_at_point(end.into()), Some(start..=end));

        let start = Point::new(1, Column(26));
        let end = Point::new(1, Column(46));
        assert_eq!(term.url_at_point(start.into()), Some(start..=end));
        assert_eq!(term.url_at_point(end.into()), Some(start..=end));
    }

    #[test]
    fn regex_right() {
        #[rustfmt::skip]
        let term = mock_term("\
            testing66\r\n\
            Alacritty\n\
            123\r\n\
            Alacritty\r\n\
            123\
        ");

        // Check regex across wrapped and unwrapped lines
        let regex = Regex::new("Ala.*123").unwrap();
        let origin = Point::new(3, Column(0));
        let start = Point::new(3, Column(0));
        let end = Point::new(2, Column(2));
        assert_eq!(term.regex_search_right(origin, &regex), Some(start..=end));
    }

    #[test]
    fn regex_left() {
        #[rustfmt::skip]
        let term = mock_term("\
            testing66\r\n\
            Alacritty\n\
            123\r\n\
            Alacritty\r\n\
            123\
        ");

        // Check regex across wrapped and unwrapped lines
        let regex = Regex::new("Ala.*123").unwrap();
        let origin = Point::new(0, Column(2));
        let start = Point::new(3, Column(0));
        let end = Point::new(2, Column(2));
        assert_eq!(term.regex_search_left(origin, &regex), Some(start..=end));
    }

    #[test]
    fn nested_regex() {
        #[rustfmt::skip]
        let term = mock_term("\
            Ala -> Alacritty -> critty\r\n\
            critty\
        ");

        // Greedy stopped at linebreak
        let regex = Regex::new("Ala.*critty").unwrap();
        let start = Point::new(1, Column(0));
        let end = Point::new(1, Column(25));
        assert_eq!(term.regex_search_right(start, &regex), Some(start..=end));

        // Greedy stopped at dead state
        let regex = Regex::new("Ala[^y]*critty").unwrap();
        let start = Point::new(1, Column(0));
        let end = Point::new(1, Column(15));
        assert_eq!(term.regex_search_right(start, &regex), Some(start..=end));
    }
}
