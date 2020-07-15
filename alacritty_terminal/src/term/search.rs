use std::cmp::min;
use std::mem;
use std::ops::RangeInclusive;

use regex_automata::{dense, DenseDFA, Error as RegexError, DFA};

use crate::grid::{BidirectionalIterator, Dimensions, GridIterator};
use crate::index::{Boundary, Column, Direction, Point, Side};
use crate::term::cell::{Cell, Flags};
use crate::term::Term;

/// Used to match equal brackets, when performing a bracket-pair selection.
const BRACKET_PAIRS: [(char, char); 4] = [('(', ')'), ('[', ']'), ('{', '}'), ('<', '>')];

pub type Match = RangeInclusive<Point<usize>>;

/// Terminal regex search state.
pub struct RegexSearch {
    /// Locate end of match searching right.
    right_fdfa: DenseDFA<Vec<usize>, usize>,
    /// Locate start of match searching right.
    right_rdfa: DenseDFA<Vec<usize>, usize>,

    /// Locate start of match searching left.
    left_fdfa: DenseDFA<Vec<usize>, usize>,
    /// Locate end of match searching left.
    left_rdfa: DenseDFA<Vec<usize>, usize>,
}

impl RegexSearch {
    /// Build the forward and backward search DFAs.
    pub fn new(search: &str) -> Result<RegexSearch, RegexError> {
        // Check case info for smart case
        let has_uppercase = search.chars().any(|c| c.is_uppercase());

        // Create Regex DFAs for all search directions.
        let mut builder = dense::Builder::new();
        let builder = builder.case_insensitive(!has_uppercase);

        let left_fdfa = builder.clone().reverse(true).build(search)?;
        let left_rdfa = builder.clone().anchored(true).longest_match(true).build(search)?;

        let right_fdfa = builder.clone().build(search)?;
        let right_rdfa = builder.anchored(true).longest_match(true).reverse(true).build(search)?;

        Ok(RegexSearch { right_fdfa, right_rdfa, left_fdfa, left_rdfa })
    }
}

impl<T> Term<T> {
    /// Enter terminal buffer search mode.
    #[inline]
    pub fn start_search(&mut self, search: &str) {
        self.regex_search = RegexSearch::new(search).ok();
        self.dirty = true;
    }

    /// Cancel active terminal buffer search.
    #[inline]
    pub fn cancel_search(&mut self) {
        self.regex_search = None;
        self.dirty = true;
    }

    /// Get next search match in the specified direction.
    pub fn search_next(
        &self,
        mut origin: Point<usize>,
        direction: Direction,
        side: Side,
        mut max_lines: Option<usize>,
    ) -> Option<Match> {
        origin = self.expand_wide(origin, direction);

        max_lines = max_lines.filter(|max_lines| max_lines + 1 < self.total_lines());

        match direction {
            Direction::Right => self.next_match_right(origin, side, max_lines),
            Direction::Left => self.next_match_left(origin, side, max_lines),
        }
    }

    /// Find the next match to the right of the origin.
    fn next_match_right(
        &self,
        origin: Point<usize>,
        side: Side,
        max_lines: Option<usize>,
    ) -> Option<Match> {
        // Skip origin itself to exclude it from the search results.
        let origin = origin.add_absolute(self, Boundary::Wrap, 1);
        let start = self.line_search_left(origin);
        let mut end = start;

        // Limit maximum number of lines searched.
        let total_lines = self.total_lines();
        end = match max_lines {
            Some(max_lines) => {
                let line = (start.line + total_lines - max_lines) % total_lines;
                Point::new(line, self.cols() - 1)
            },
            _ => end.sub_absolute(self, Boundary::Wrap, 1),
        };

        let mut regex_iter = RegexIter::new(start, end, Direction::Right, &self).peekable();

        // Check if there's any match at all.
        let first_match = regex_iter.peek()?.clone();

        let regex_match = regex_iter
            .find(|regex_match| {
                let match_point = Self::match_side(&regex_match, side);

                // If the match's point is beyond the origin, we're done.
                match_point.line > start.line
                    || match_point.line < origin.line
                    || (match_point.line == origin.line && match_point.col >= origin.col)
            })
            .unwrap_or(first_match);

        Some(regex_match)
    }

    /// Find the next match to the left of the origin.
    fn next_match_left(
        &self,
        origin: Point<usize>,
        side: Side,
        max_lines: Option<usize>,
    ) -> Option<Match> {
        // Skip origin itself to exclude it from the search results.
        let origin = origin.sub_absolute(self, Boundary::Wrap, 1);
        let start = self.line_search_right(origin);
        let mut end = start;

        // Limit maximum number of lines searched.
        end = match max_lines {
            Some(max_lines) => Point::new((start.line + max_lines) % self.total_lines(), Column(0)),
            _ => end.add_absolute(self, Boundary::Wrap, 1),
        };

        let mut regex_iter = RegexIter::new(start, end, Direction::Left, &self).peekable();

        // Check if there's any match at all.
        let first_match = regex_iter.peek()?.clone();

        let regex_match = regex_iter
            .find(|regex_match| {
                let match_point = Self::match_side(&regex_match, side);

                // If the match's point is beyond the origin, we're done.
                match_point.line < start.line
                    || match_point.line > origin.line
                    || (match_point.line == origin.line && match_point.col <= origin.col)
            })
            .unwrap_or(first_match);

        Some(regex_match)
    }

    /// Get the side of a match.
    fn match_side(regex_match: &Match, side: Side) -> Point<usize> {
        match side {
            Side::Right => *regex_match.end(),
            Side::Left => *regex_match.start(),
        }
    }

    /// Find the next regex match to the left of the origin point.
    ///
    /// The origin is always included in the regex.
    pub fn regex_search_left(&self, start: Point<usize>, end: Point<usize>) -> Option<Match> {
        let RegexSearch { left_fdfa: fdfa, left_rdfa: rdfa, .. } = self.regex_search.as_ref()?;

        // Find start and end of match.
        let match_start = self.regex_search(start, end, Direction::Left, &fdfa)?;
        let match_end = self.regex_search(match_start, start, Direction::Right, &rdfa)?;

        Some(match_start..=match_end)
    }

    /// Find the next regex match to the right of the origin point.
    ///
    /// The origin is always included in the regex.
    pub fn regex_search_right(&self, start: Point<usize>, end: Point<usize>) -> Option<Match> {
        let RegexSearch { right_fdfa: fdfa, right_rdfa: rdfa, .. } = self.regex_search.as_ref()?;

        // Find start and end of match.
        let match_end = self.regex_search(start, end, Direction::Right, &fdfa)?;
        let match_start = self.regex_search(match_end, start, Direction::Left, &rdfa)?;

        Some(match_start..=match_end)
    }

    /// Find the next regex match.
    ///
    /// This will always return the side of the first match which is farthest from the start point.
    fn regex_search(
        &self,
        start: Point<usize>,
        end: Point<usize>,
        direction: Direction,
        dfa: &impl DFA,
    ) -> Option<Point<usize>> {
        let last_line = self.total_lines() - 1;
        let last_col = self.cols() - 1;

        // Advance the iterator.
        let next = match direction {
            Direction::Right => GridIterator::next,
            Direction::Left => GridIterator::prev,
        };

        let mut iter = self.grid.iter_from(start);
        let mut state = dfa.start_state();
        let mut regex_match = None;

        let mut cell = *iter.cell();
        self.skip_fullwidth(&mut iter, &mut cell, direction);
        let mut point = iter.point();

        loop {
            // Convert char to array of bytes.
            let mut buf = [0; 4];
            let utf8_len = cell.c.encode_utf8(&mut buf).len();

            // Pass char to DFA as individual bytes.
            for i in 0..utf8_len {
                // Inverse byte order when going left.
                let byte = match direction {
                    Direction::Right => buf[i],
                    Direction::Left => buf[utf8_len - i - 1],
                };

                // Since we get the state from the DFA, it doesn't need to be checked.
                state = unsafe { dfa.next_state_unchecked(state, byte) };
            }

            // Handle regex state changes.
            if dfa.is_match_or_dead_state(state) {
                if dfa.is_dead_state(state) {
                    break;
                } else {
                    regex_match = Some(point);
                }
            }

            // Stop once we've reached the target point.
            if point == end {
                break;
            }

            // Advance grid cell iterator.
            let mut new_cell = match next(&mut iter) {
                Some(&cell) => cell,
                None => {
                    // Wrap around to other end of the scrollback buffer.
                    let start = Point::new(last_line - point.line, last_col - point.col);
                    iter = self.grid.iter_from(start);
                    *iter.cell()
                },
            };
            self.skip_fullwidth(&mut iter, &mut new_cell, direction);
            let last_point = mem::replace(&mut point, iter.point());
            let last_cell = mem::replace(&mut cell, new_cell);

            // Handle linebreaks.
            if (last_point.col == last_col
                && point.col == Column(0)
                && !last_cell.flags.contains(Flags::WRAPLINE))
                || (last_point.col == Column(0)
                    && point.col == last_col
                    && !cell.flags.contains(Flags::WRAPLINE))
            {
                match regex_match {
                    Some(_) => break,
                    None => state = dfa.start_state(),
                }
            }
        }

        regex_match
    }

    /// Advance a grid iterator over fullwidth characters.
    fn skip_fullwidth(
        &self,
        iter: &mut GridIterator<'_, Cell>,
        cell: &mut Cell,
        direction: Direction,
    ) {
        match direction {
            Direction::Right if cell.flags.contains(Flags::WIDE_CHAR) => {
                iter.next();
            },
            Direction::Right if cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) => {
                if let Some(new_cell) = iter.next() {
                    *cell = *new_cell;
                }
                iter.next();
            },
            Direction::Left if cell.flags.contains(Flags::WIDE_CHAR_SPACER) => {
                if let Some(new_cell) = iter.prev() {
                    *cell = *new_cell;
                }

                let prev = iter.point().sub_absolute(self, Boundary::Clamp, 1);
                if self.grid[prev].flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
                    iter.prev();
                }
            },
            _ => (),
        }
    }

    /// Find next matching bracket.
    pub fn bracket_search(&self, point: Point<usize>) -> Option<Point<usize>> {
        let start_char = self.grid[point.line][point.col].c;

        // Find the matching bracket we're looking for
        let (forward, end_char) = BRACKET_PAIRS.iter().find_map(|(open, close)| {
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
            let cell = if forward { iter.next() } else { iter.prev() };

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

    /// Find left end of semantic block.
    pub fn semantic_search_left(&self, mut point: Point<usize>) -> Point<usize> {
        // Limit the starting point to the last line in the history
        point.line = min(point.line, self.total_lines() - 1);

        let mut iter = self.grid.iter_from(point);
        let last_col = self.cols() - Column(1);

        let wide = Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER;
        while let Some(cell) = iter.prev() {
            if !cell.flags.intersects(wide) && self.semantic_escape_chars.contains(cell.c) {
                break;
            }

            if iter.point().col == last_col && !cell.flags.contains(Flags::WRAPLINE) {
                break; // cut off if on new line or hit escape char
            }

            point = iter.point();
        }

        point
    }

    /// Find right end of semantic block.
    pub fn semantic_search_right(&self, mut point: Point<usize>) -> Point<usize> {
        // Limit the starting point to the last line in the history
        point.line = min(point.line, self.total_lines() - 1);

        let mut iter = self.grid.iter_from(point);
        let last_col = self.cols() - 1;

        let wide = Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER;
        while let Some(cell) = iter.next() {
            if !cell.flags.intersects(wide) && self.semantic_escape_chars.contains(cell.c) {
                break;
            }

            point = iter.point();

            if point.col == last_col && !cell.flags.contains(Flags::WRAPLINE) {
                break; // cut off if on new line or hit escape char
            }
        }

        point
    }

    /// Find the beginning of the current line across linewraps.
    pub fn line_search_left(&self, mut point: Point<usize>) -> Point<usize> {
        while point.line + 1 < self.total_lines()
            && self.grid[point.line + 1][self.cols() - 1].flags.contains(Flags::WRAPLINE)
        {
            point.line += 1;
        }

        point.col = Column(0);

        point
    }

    /// Find the end of the current line across linewraps.
    pub fn line_search_right(&self, mut point: Point<usize>) -> Point<usize> {
        while self.grid[point.line][self.cols() - 1].flags.contains(Flags::WRAPLINE) {
            point.line -= 1;
        }

        point.col = self.cols() - 1;

        point
    }
}

/// Iterator over regex matches.
pub struct RegexIter<'a, T> {
    point: Point<usize>,
    end: Point<usize>,
    direction: Direction,
    term: &'a Term<T>,
    done: bool,
}

impl<'a, T> RegexIter<'a, T> {
    pub fn new(
        start: Point<usize>,
        end: Point<usize>,
        direction: Direction,
        term: &'a Term<T>,
    ) -> Self {
        Self { point: start, done: false, end, direction, term }
    }

    /// Skip one cell, advancing the origin point to the next one.
    fn skip(&mut self) {
        self.point = self.term.expand_wide(self.point, self.direction);

        self.point = match self.direction {
            Direction::Right => self.point.add_absolute(self.term, Boundary::Wrap, 1),
            Direction::Left => self.point.sub_absolute(self.term, Boundary::Wrap, 1),
        };
    }

    /// Get the next match in the specified direction.
    fn next_match(&self) -> Option<Match> {
        match self.direction {
            Direction::Right => self.term.regex_search_right(self.point, self.end),
            Direction::Left => self.term.regex_search_left(self.point, self.end),
        }
    }
}

impl<'a, T> Iterator for RegexIter<'a, T> {
    type Item = Match;

    fn next(&mut self) -> Option<Self::Item> {
        if self.point == self.end {
            self.done = true;
        } else if self.done {
            return None;
        }

        let regex_match = self.next_match()?;

        self.point = *regex_match.end();
        self.skip();

        Some(regex_match)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::index::Column;
    use crate::term::test::mock_term;

    #[test]
    fn regex_right() {
        #[rustfmt::skip]
        let mut term = mock_term("\
            testing66\r\n\
            Alacritty\n\
            123\r\n\
            Alacritty\r\n\
            123\
        ");

        // Check regex across wrapped and unwrapped lines.
        term.regex_search = Some(RegexSearch::new("Ala.*123").unwrap());
        let start = Point::new(3, Column(0));
        let end = Point::new(0, Column(2));
        let match_start = Point::new(3, Column(0));
        let match_end = Point::new(2, Column(2));
        assert_eq!(term.regex_search_right(start, end), Some(match_start..=match_end));
    }

    #[test]
    fn regex_left() {
        #[rustfmt::skip]
        let mut term = mock_term("\
            testing66\r\n\
            Alacritty\n\
            123\r\n\
            Alacritty\r\n\
            123\
        ");

        // Check regex across wrapped and unwrapped lines.
        term.regex_search = Some(RegexSearch::new("Ala.*123").unwrap());
        let start = Point::new(0, Column(2));
        let end = Point::new(3, Column(0));
        let match_start = Point::new(3, Column(0));
        let match_end = Point::new(2, Column(2));
        assert_eq!(term.regex_search_left(start, end), Some(match_start..=match_end));
    }

    #[test]
    fn nested_regex() {
        #[rustfmt::skip]
        let mut term = mock_term("\
            Ala -> Alacritty -> critty\r\n\
            critty\
        ");

        // Greedy stopped at linebreak.
        term.regex_search = Some(RegexSearch::new("Ala.*critty").unwrap());
        let start = Point::new(1, Column(0));
        let end = Point::new(1, Column(25));
        assert_eq!(term.regex_search_right(start, end), Some(start..=end));

        // Greedy stopped at dead state.
        term.regex_search = Some(RegexSearch::new("Ala[^y]*critty").unwrap());
        let start = Point::new(1, Column(0));
        let end = Point::new(1, Column(15));
        assert_eq!(term.regex_search_right(start, end), Some(start..=end));
    }

    #[test]
    fn no_match_right() {
        #[rustfmt::skip]
        let mut term = mock_term("\
            first line\n\
            broken second\r\n\
            third\
        ");

        term.regex_search = Some(RegexSearch::new("nothing").unwrap());
        let start = Point::new(2, Column(0));
        let end = Point::new(0, Column(4));
        assert_eq!(term.regex_search_right(start, end), None);
    }

    #[test]
    fn no_match_left() {
        #[rustfmt::skip]
        let mut term = mock_term("\
            first line\n\
            broken second\r\n\
            third\
        ");

        term.regex_search = Some(RegexSearch::new("nothing").unwrap());
        let start = Point::new(0, Column(4));
        let end = Point::new(2, Column(0));
        assert_eq!(term.regex_search_left(start, end), None);
    }

    #[test]
    fn include_linebreak_left() {
        #[rustfmt::skip]
        let mut term = mock_term("\
            testing123\r\n\
            xxx\
        ");

        // Make sure the cell containing the linebreak is not skipped.
        term.regex_search = Some(RegexSearch::new("te.*123").unwrap());
        let start = Point::new(0, Column(0));
        let end = Point::new(1, Column(0));
        let match_start = Point::new(1, Column(0));
        let match_end = Point::new(1, Column(9));
        assert_eq!(term.regex_search_left(start, end), Some(match_start..=match_end));
    }

    #[test]
    fn include_linebreak_right() {
        #[rustfmt::skip]
        let mut term = mock_term("\
            xxx\r\n\
            testing123\
        ");

        // Make sure the cell containing the linebreak is not skipped.
        term.regex_search = Some(RegexSearch::new("te.*123").unwrap());
        let start = Point::new(1, Column(2));
        let end = Point::new(0, Column(9));
        let match_start = Point::new(0, Column(0));
        assert_eq!(term.regex_search_right(start, end), Some(match_start..=end));
    }

    #[test]
    fn skip_dead_cell() {
        let mut term = mock_term("alacritty");

        // Make sure dead state cell is skipped when reversing.
        term.regex_search = Some(RegexSearch::new("alacrit").unwrap());
        let start = Point::new(0, Column(0));
        let end = Point::new(0, Column(6));
        assert_eq!(term.regex_search_right(start, end), Some(start..=end));
    }

    #[test]
    fn reverse_search_dead_recovery() {
        let mut term = mock_term("zooo lense");

        // Make sure the reverse DFA operates the same as a forward DFA.
        term.regex_search = Some(RegexSearch::new("zoo").unwrap());
        let start = Point::new(0, Column(9));
        let end = Point::new(0, Column(0));
        let match_start = Point::new(0, Column(0));
        let match_end = Point::new(0, Column(2));
        assert_eq!(term.regex_search_left(start, end), Some(match_start..=match_end));
    }

    #[test]
    fn multibyte_unicode() {
        let mut term = mock_term("testвосибing");

        term.regex_search = Some(RegexSearch::new("te.*ing").unwrap());
        let start = Point::new(0, Column(0));
        let end = Point::new(0, Column(11));
        assert_eq!(term.regex_search_right(start, end), Some(start..=end));

        term.regex_search = Some(RegexSearch::new("te.*ing").unwrap());
        let start = Point::new(0, Column(11));
        let end = Point::new(0, Column(0));
        assert_eq!(term.regex_search_left(start, end), Some(end..=start));
    }

    #[test]
    fn fullwidth() {
        let mut term = mock_term("a🦇x🦇");

        term.regex_search = Some(RegexSearch::new("[^ ]*").unwrap());
        let start = Point::new(0, Column(0));
        let end = Point::new(0, Column(5));
        assert_eq!(term.regex_search_right(start, end), Some(start..=end));

        term.regex_search = Some(RegexSearch::new("[^ ]*").unwrap());
        let start = Point::new(0, Column(5));
        let end = Point::new(0, Column(0));
        assert_eq!(term.regex_search_left(start, end), Some(end..=start));
    }

    #[test]
    fn singlecell_fullwidth() {
        let mut term = mock_term("🦇");

        term.regex_search = Some(RegexSearch::new("🦇").unwrap());
        let start = Point::new(0, Column(0));
        let end = Point::new(0, Column(1));
        assert_eq!(term.regex_search_right(start, end), Some(start..=end));

        term.regex_search = Some(RegexSearch::new("🦇").unwrap());
        let start = Point::new(0, Column(1));
        let end = Point::new(0, Column(0));
        assert_eq!(term.regex_search_left(start, end), Some(end..=start));
    }

    #[test]
    fn wrapping() {
        #[rustfmt::skip]
        let mut term = mock_term("\
            xxx\r\n\
            xxx\
        ");

        term.regex_search = Some(RegexSearch::new("xxx").unwrap());
        let start = Point::new(0, Column(2));
        let end = Point::new(1, Column(2));
        let match_start = Point::new(1, Column(0));
        assert_eq!(term.regex_search_right(start, end), Some(match_start..=end));

        term.regex_search = Some(RegexSearch::new("xxx").unwrap());
        let start = Point::new(1, Column(0));
        let end = Point::new(0, Column(0));
        let match_end = Point::new(0, Column(2));
        assert_eq!(term.regex_search_left(start, end), Some(end..=match_end));
    }

    #[test]
    fn wrapping_into_fullwidth() {
        #[rustfmt::skip]
        let mut term = mock_term("\
            🦇xx\r\n\
            xx🦇\
        ");

        term.regex_search = Some(RegexSearch::new("🦇x").unwrap());
        let start = Point::new(0, Column(0));
        let end = Point::new(1, Column(3));
        let match_start = Point::new(1, Column(0));
        let match_end = Point::new(1, Column(2));
        assert_eq!(term.regex_search_right(start, end), Some(match_start..=match_end));

        term.regex_search = Some(RegexSearch::new("x🦇").unwrap());
        let start = Point::new(1, Column(2));
        let end = Point::new(0, Column(0));
        let match_start = Point::new(0, Column(1));
        let match_end = Point::new(0, Column(3));
        assert_eq!(term.regex_search_left(start, end), Some(match_start..=match_end));
    }

    #[test]
    fn leading_spacer() {
        #[rustfmt::skip]
        let mut term = mock_term("\
            xxx \n\
            🦇xx\
        ");
        term.grid[1][Column(3)].flags.insert(Flags::LEADING_WIDE_CHAR_SPACER);

        term.regex_search = Some(RegexSearch::new("🦇x").unwrap());
        let start = Point::new(1, Column(0));
        let end = Point::new(0, Column(3));
        let match_start = Point::new(1, Column(3));
        let match_end = Point::new(0, Column(2));
        assert_eq!(term.regex_search_right(start, end), Some(match_start..=match_end));

        term.regex_search = Some(RegexSearch::new("🦇x").unwrap());
        let start = Point::new(0, Column(3));
        let end = Point::new(1, Column(0));
        let match_start = Point::new(1, Column(3));
        let match_end = Point::new(0, Column(2));
        assert_eq!(term.regex_search_left(start, end), Some(match_start..=match_end));

        term.regex_search = Some(RegexSearch::new("x🦇").unwrap());
        let start = Point::new(1, Column(0));
        let end = Point::new(0, Column(3));
        let match_start = Point::new(1, Column(2));
        let match_end = Point::new(0, Column(1));
        assert_eq!(term.regex_search_right(start, end), Some(match_start..=match_end));

        term.regex_search = Some(RegexSearch::new("x🦇").unwrap());
        let start = Point::new(0, Column(3));
        let end = Point::new(1, Column(0));
        let match_start = Point::new(1, Column(2));
        let match_end = Point::new(0, Column(1));
        assert_eq!(term.regex_search_left(start, end), Some(match_start..=match_end));
    }
}

#[cfg(all(test, feature = "bench"))]
mod benches {
    extern crate test;

    use super::*;

    use crate::term::test::mock_term;

    #[bench]
    fn regex_search(b: &mut test::Bencher) {
        let input = format!("{:^10000}", "Alacritty");
        let mut term = mock_term(&input);
        term.regex_search = Some(RegexSearch::new("   Alacritty   ").unwrap());
        let start = Point::new(0, Column(0));
        let end = Point::new(0, Column(input.len() - 1));

        b.iter(|| {
            test::black_box(term.regex_search_right(start, end));
            test::black_box(term.regex_search_left(end, start));
        });
    }
}
