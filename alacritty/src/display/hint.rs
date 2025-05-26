use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::HashSet;
use std::iter;
use std::rc::Rc;

use ahash::RandomState;
use winit::keyboard::ModifiersState;

use alacritty_terminal::grid::{BidirectionalIterator, Dimensions};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point};
use alacritty_terminal::term::cell::Hyperlink;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::term::{Term, TermMode};

use crate::config::UiConfig;
use crate::config::ui_config::{Hint, HintAction};

/// Maximum number of linewraps followed outside of the viewport during search highlighting.
pub const MAX_SEARCH_LINES: usize = 100;

/// Percentage of characters in the hints alphabet used for the last character.
const HINT_SPLIT_PERCENTAGE: f32 = 0.5;

/// Keyboard regex hint state.
pub struct HintState {
    /// Hint currently in use.
    hint: Option<Rc<Hint>>,

    /// Alphabet for hint labels.
    alphabet: String,

    /// Visible matches.
    matches: Vec<Match>,

    /// Key label for each visible match.
    labels: Vec<Vec<char>>,

    /// Keys pressed for hint selection.
    keys: Vec<char>,
}

impl HintState {
    /// Initialize an inactive hint state.
    pub fn new<S: Into<String>>(alphabet: S) -> Self {
        Self {
            alphabet: alphabet.into(),
            hint: Default::default(),
            matches: Default::default(),
            labels: Default::default(),
            keys: Default::default(),
        }
    }

    /// Check if a hint selection is in progress.
    pub fn active(&self) -> bool {
        self.hint.is_some()
    }

    /// Start the hint selection process.
    pub fn start(&mut self, hint: Rc<Hint>) {
        self.hint = Some(hint);
    }

    /// Cancel the hint highlighting process.
    fn stop(&mut self) {
        self.matches.clear();
        self.labels.clear();
        self.keys.clear();
        self.hint = None;
    }

    /// Update the visible hint matches and key labels.
    pub fn update_matches<T>(&mut self, term: &Term<T>) {
        let hint = match self.hint.as_mut() {
            Some(hint) => hint,
            None => return,
        };

        // Clear current matches.
        self.matches.clear();

        // Add escape sequence hyperlinks.
        if hint.content.hyperlinks {
            self.matches.extend(visible_unique_hyperlinks_iter(term));
        }

        // Add visible regex matches.
        if let Some(regex) = hint.content.regex.as_ref() {
            regex.with_compiled(|regex| {
                let matches = visible_regex_match_iter(term, regex);

                // Apply post-processing and search for sub-matches if necessary.
                if hint.post_processing {
                    let mut matches = matches.collect::<Vec<_>>();
                    self.matches.extend(matches.drain(..).flat_map(|rm| {
                        HintPostProcessor::new(term, regex, rm).collect::<Vec<_>>()
                    }));
                } else {
                    self.matches.extend(matches);
                }
            });
        }

        // Cancel highlight with no visible matches.
        if self.matches.is_empty() {
            self.stop();
            return;
        }

        // Sort and dedup ranges. Currently overlapped but not exactly same ranges are kept.
        self.matches.sort_by_key(|bounds| (*bounds.start(), Reverse(*bounds.end())));
        self.matches.dedup_by_key(|bounds| *bounds.start());

        let mut generator = HintLabels::new(&self.alphabet, HINT_SPLIT_PERCENTAGE);
        let match_count = self.matches.len();
        let keys_len = self.keys.len();

        // Get the label for each match.
        self.labels.resize(match_count, Vec::new());
        for i in (0..match_count).rev() {
            let mut label = generator.next();
            if label.len() >= keys_len && label[..keys_len] == self.keys[..] {
                self.labels[i] = label.split_off(keys_len);
            } else {
                self.labels[i] = Vec::new();
            }
        }
    }

    /// Handle keyboard input during hint selection.
    pub fn keyboard_input<T>(&mut self, term: &Term<T>, c: char) -> Option<HintMatch> {
        match c {
            // Use backspace to remove the last character pressed.
            '\x08' | '\x1f' => {
                self.keys.pop();
            },
            // Cancel hint highlighting on ESC/Ctrl+c.
            '\x1b' | '\x03' => self.stop(),
            _ => (),
        }

        // Update the visible matches.
        self.update_matches(term);

        let hint = self.hint.as_ref()?;

        // Find the last label starting with the input character.
        let mut labels = self.labels.iter().enumerate().rev();
        let (index, label) = labels.find(|(_, label)| !label.is_empty() && label[0] == c)?;

        // Check if the selected label is fully matched.
        if label.len() == 1 {
            let bounds = self.matches[index].clone();
            let hint = hint.clone();

            // Exit hint mode unless it requires explicit dismissal.
            if hint.persist {
                self.keys.clear();
            } else {
                self.stop();
            }

            // Hyperlinks take precedence over regex matches.
            let hyperlink = term.grid()[*bounds.start()].hyperlink();
            Some(HintMatch { bounds, hyperlink, hint })
        } else {
            // Store character to preserve the selection.
            self.keys.push(c);

            None
        }
    }

    /// Hint key labels.
    pub fn labels(&self) -> &Vec<Vec<char>> {
        &self.labels
    }

    /// Visible hint regex matches.
    pub fn matches(&self) -> &[Match] {
        &self.matches
    }

    /// Update the alphabet used for hint labels.
    pub fn update_alphabet(&mut self, alphabet: &str) {
        if self.alphabet != alphabet {
            alphabet.clone_into(&mut self.alphabet);
            self.keys.clear();
        }
    }
}

/// Hint match which was selected by the user.
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct HintMatch {
    /// Terminal range matching the hint.
    bounds: Match,

    /// OSC 8 hyperlink.
    hyperlink: Option<Hyperlink>,

    /// Hint which triggered this match.
    hint: Rc<Hint>,
}

impl HintMatch {
    #[inline]
    pub fn should_highlight(&self, point: Point, pointed_hyperlink: Option<&Hyperlink>) -> bool {
        self.hyperlink.as_ref() == pointed_hyperlink
            && (self.hyperlink.is_some() || self.bounds.contains(&point))
    }

    #[inline]
    pub fn action(&self) -> &HintAction {
        &self.hint.action
    }

    #[inline]
    pub fn bounds(&self) -> &Match {
        &self.bounds
    }

    pub fn hyperlink(&self) -> Option<&Hyperlink> {
        self.hyperlink.as_ref()
    }

    /// Get the text content of the hint match.
    ///
    /// This will always revalidate the hint text, to account for terminal content
    /// changes since the [`HintMatch`] was constructed. The text of the hint might
    /// be different from its original value, but it will **always** be a valid
    /// match for this hint.
    pub fn text<T>(&self, term: &Term<T>) -> Option<Cow<'_, str>> {
        // Revalidate hyperlink match.
        if let Some(hyperlink) = &self.hyperlink {
            let (validated, bounds) = hyperlink_at(term, *self.bounds.start())?;
            return (&validated == hyperlink && bounds == self.bounds)
                .then(|| hyperlink.uri().into());
        }

        // Revalidate regex match.
        let regex = self.hint.content.regex.as_ref()?;
        let bounds = regex.with_compiled(|regex| {
            regex_match_at(term, *self.bounds.start(), regex, self.hint.post_processing)
        })??;
        (bounds == self.bounds)
            .then(|| term.bounds_to_string(*bounds.start(), *bounds.end()).into())
    }
}

/// Generator for creating new hint labels.
struct HintLabels {
    /// Full character set available.
    alphabet: Vec<char>,

    /// Alphabet indices for the next label.
    indices: Vec<usize>,

    /// Point separating the alphabet's head and tail characters.
    ///
    /// To make identification of the tail character easy, part of the alphabet cannot be used for
    /// any other position.
    ///
    /// All characters in the alphabet before this index will be used for the last character, while
    /// the rest will be used for everything else.
    split_point: usize,
}

impl HintLabels {
    /// Create a new label generator.
    ///
    /// The `split_ratio` should be a number between 0.0 and 1.0 representing the percentage of
    /// elements in the alphabet which are reserved for the tail of the hint label.
    fn new(alphabet: impl Into<String>, split_ratio: f32) -> Self {
        let alphabet: Vec<char> = alphabet.into().chars().collect();
        let split_point = ((alphabet.len() - 1) as f32 * split_ratio.min(1.)) as usize;

        Self { indices: vec![0], split_point, alphabet }
    }

    /// Get the characters for the next label.
    fn next(&mut self) -> Vec<char> {
        let characters = self.indices.iter().rev().map(|index| self.alphabet[*index]).collect();
        self.increment();
        characters
    }

    /// Increment the character sequence.
    fn increment(&mut self) {
        // Increment the last character; if it's not at the split point we're done.
        let tail = &mut self.indices[0];
        if *tail < self.split_point {
            *tail += 1;
            return;
        }
        *tail = 0;

        // Increment all other characters in reverse order.
        let alphabet_len = self.alphabet.len();
        for index in self.indices.iter_mut().skip(1) {
            if *index + 1 == alphabet_len {
                // Reset character and move to the next if it's already at the limit.
                *index = self.split_point + 1;
            } else {
                // If the character can be incremented, we're done.
                *index += 1;
                return;
            }
        }

        // Extend the sequence with another character when nothing could be incremented.
        self.indices.push(self.split_point + 1);
    }
}

/// Iterate over all visible regex matches.
pub fn visible_regex_match_iter<'a, T>(
    term: &'a Term<T>,
    regex: &'a mut RegexSearch,
) -> impl Iterator<Item = Match> + 'a {
    let viewport_start = Line(-(term.grid().display_offset() as i32));
    let viewport_end = viewport_start + term.bottommost_line();
    let mut start = term.line_search_left(Point::new(viewport_start, Column(0)));
    let mut end = term.line_search_right(Point::new(viewport_end, Column(0)));
    start.line = start.line.max(viewport_start - MAX_SEARCH_LINES);
    end.line = end.line.min(viewport_end + MAX_SEARCH_LINES);

    RegexIter::new(start, end, Direction::Right, term, regex)
        .skip_while(move |rm| rm.end().line < viewport_start)
        .take_while(move |rm| rm.start().line <= viewport_end)
}

/// Iterate over all visible hyperlinks, yanking only unique ones.
pub fn visible_unique_hyperlinks_iter<T>(term: &Term<T>) -> impl Iterator<Item = Match> + '_ {
    let mut display_iter = term.grid().display_iter().peekable();

    // Avoid creating hints for the same hyperlinks, but from a different places.
    let mut unique_hyperlinks = HashSet::<Hyperlink, RandomState>::default();

    iter::from_fn(move || {
        // Find the start of the next unique hyperlink.
        let (cell, hyperlink) = display_iter.find_map(|cell| {
            let hyperlink = cell.hyperlink()?;
            (!unique_hyperlinks.contains(&hyperlink)).then(|| {
                unique_hyperlinks.insert(hyperlink.clone());
                (cell, hyperlink)
            })
        })?;

        let start = cell.point;
        let mut end = start;

        // Find the end bound of just found unique hyperlink.
        while let Some(next_cell) = display_iter.peek() {
            // Cell at display iter doesn't match, yield the hyperlink and start over with
            // `find_map`.
            if next_cell.hyperlink().as_ref() != Some(&hyperlink) {
                break;
            }

            // Advance to the next cell.
            end = next_cell.point;
            let _ = display_iter.next();
        }

        Some(start..=end)
    })
}

/// Retrieve the match, if the specified point is inside the content matching the regex.
fn regex_match_at<T>(
    term: &Term<T>,
    point: Point,
    regex: &mut RegexSearch,
    post_processing: bool,
) -> Option<Match> {
    let regex_match = visible_regex_match_iter(term, regex).find(|rm| rm.contains(&point))?;

    // Apply post-processing and search for sub-matches if necessary.
    if post_processing {
        HintPostProcessor::new(term, regex, regex_match).find(|rm| rm.contains(&point))
    } else {
        Some(regex_match)
    }
}

/// Check if there is a hint highlighted at the specified point.
pub fn highlighted_at<T>(
    term: &Term<T>,
    config: &UiConfig,
    point: Point,
    mouse_mods: ModifiersState,
) -> Option<HintMatch> {
    let mouse_mode = term.mode().intersects(TermMode::MOUSE_MODE);

    config.hints.enabled.iter().find_map(|hint| {
        // Check if all required modifiers are pressed.
        let highlight = hint.mouse.is_some_and(|mouse| {
            mouse.enabled
                && mouse_mods.contains(mouse.mods.0)
                && (!mouse_mode || mouse_mods.contains(ModifiersState::SHIFT))
        });
        if !highlight {
            return None;
        }

        if let Some((hyperlink, bounds)) =
            hint.content.hyperlinks.then(|| hyperlink_at(term, point)).flatten()
        {
            return Some(HintMatch { bounds, hyperlink: Some(hyperlink), hint: hint.clone() });
        }

        let bounds = hint.content.regex.as_ref().and_then(|regex| {
            regex.with_compiled(|regex| regex_match_at(term, point, regex, hint.post_processing))
        });
        if let Some(bounds) = bounds.flatten() {
            return Some(HintMatch { bounds, hint: hint.clone(), hyperlink: None });
        }

        None
    })
}

/// Retrieve the hyperlink with its range, if there is one at the specified point.
///
/// This will only return contiguous cells, even if another hyperlink with the same ID exists.
fn hyperlink_at<T>(term: &Term<T>, point: Point) -> Option<(Hyperlink, Match)> {
    let hyperlink = term.grid()[point].hyperlink()?;

    let grid = term.grid();

    let mut match_end = point;
    for cell in grid.iter_from(point) {
        if cell.hyperlink().is_some_and(|link| link == hyperlink) {
            match_end = cell.point;
        } else {
            break;
        }
    }

    let mut match_start = point;
    let mut iter = grid.iter_from(point);
    while let Some(cell) = iter.prev() {
        if cell.hyperlink().is_some_and(|link| link == hyperlink) {
            match_start = cell.point;
        } else {
            break;
        }
    }

    Some((hyperlink, match_start..=match_end))
}

/// Iterator over all post-processed matches inside an existing hint match.
struct HintPostProcessor<'a, T> {
    /// Regex search DFAs.
    regex: &'a mut RegexSearch,

    /// Terminal reference.
    term: &'a Term<T>,

    /// Next hint match in the iterator.
    next_match: Option<Match>,

    /// Start point for the next search.
    start: Point,

    /// End point for the hint match iterator.
    end: Point,
}

impl<'a, T> HintPostProcessor<'a, T> {
    /// Create a new iterator for an unprocessed match.
    fn new(term: &'a Term<T>, regex: &'a mut RegexSearch, regex_match: Match) -> Self {
        let mut post_processor = Self {
            next_match: None,
            start: *regex_match.start(),
            end: *regex_match.end(),
            term,
            regex,
        };

        // Post-process the first hint match.
        post_processor.next_processed_match(regex_match);

        post_processor
    }

    /// Apply some hint post processing heuristics.
    ///
    /// This will check the end of the hint and make it shorter if certain characters are determined
    /// to be unlikely to be intentionally part of the hint.
    ///
    /// This is most useful for identifying URLs appropriately.
    fn hint_post_processing(&self, regex_match: &Match) -> Option<Match> {
        let mut iter = self.term.grid().iter_from(*regex_match.start());

        let mut c = iter.cell().c;

        // Truncate uneven number of brackets.
        let end = *regex_match.end();
        let mut open_parents = 0;
        let mut open_brackets = 0;
        loop {
            match c {
                '(' => open_parents += 1,
                '[' => open_brackets += 1,
                ')' => {
                    if open_parents == 0 {
                        iter.prev();
                        break;
                    } else {
                        open_parents -= 1;
                    }
                },
                ']' => {
                    if open_brackets == 0 {
                        iter.prev();
                        break;
                    } else {
                        open_brackets -= 1;
                    }
                },
                _ => (),
            }

            if iter.point() == end {
                break;
            }

            match iter.next() {
                Some(indexed) => c = indexed.cell.c,
                None => break,
            }
        }

        // Truncate trailing characters which are likely to be delimiters.
        let start = *regex_match.start();
        while iter.point() != start {
            if !matches!(c, '.' | ',' | ':' | ';' | '?' | '!' | '(' | '[' | '\'') {
                break;
            }

            match iter.prev() {
                Some(indexed) => c = indexed.cell.c,
                None => break,
            }
        }

        if start > iter.point() { None } else { Some(start..=iter.point()) }
    }

    /// Loop over submatches until a non-empty post-processed match is found.
    fn next_processed_match(&mut self, mut regex_match: Match) {
        self.next_match = loop {
            if let Some(next_match) = self.hint_post_processing(&regex_match) {
                self.start = next_match.end().add(self.term, Boundary::Grid, 1);
                break Some(next_match);
            }

            self.start = regex_match.start().add(self.term, Boundary::Grid, 1);
            if self.start > self.end {
                return;
            }

            match self.term.regex_search_right(self.regex, self.start, self.end) {
                Some(rm) => regex_match = rm,
                None => return,
            }
        };
    }
}

impl<T> Iterator for HintPostProcessor<'_, T> {
    type Item = Match;

    fn next(&mut self) -> Option<Self::Item> {
        let next_match = self.next_match.take()?;

        if self.start <= self.end {
            if let Some(rm) = self.term.regex_search_right(self.regex, self.start, self.end) {
                self.next_processed_match(rm);
            }
        }

        Some(next_match)
    }
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::test::mock_term;
    use alacritty_terminal::vte::ansi::Handler;

    use super::*;

    #[test]
    fn hint_label_generation() {
        let mut generator = HintLabels::new("0123", 0.5);

        assert_eq!(generator.next(), vec!['0']);
        assert_eq!(generator.next(), vec!['1']);

        assert_eq!(generator.next(), vec!['2', '0']);
        assert_eq!(generator.next(), vec!['2', '1']);
        assert_eq!(generator.next(), vec!['3', '0']);
        assert_eq!(generator.next(), vec!['3', '1']);

        assert_eq!(generator.next(), vec!['2', '2', '0']);
        assert_eq!(generator.next(), vec!['2', '2', '1']);
        assert_eq!(generator.next(), vec!['2', '3', '0']);
        assert_eq!(generator.next(), vec!['2', '3', '1']);
        assert_eq!(generator.next(), vec!['3', '2', '0']);
        assert_eq!(generator.next(), vec!['3', '2', '1']);
        assert_eq!(generator.next(), vec!['3', '3', '0']);
        assert_eq!(generator.next(), vec!['3', '3', '1']);

        assert_eq!(generator.next(), vec!['2', '2', '2', '0']);
        assert_eq!(generator.next(), vec!['2', '2', '2', '1']);
        assert_eq!(generator.next(), vec!['2', '2', '3', '0']);
        assert_eq!(generator.next(), vec!['2', '2', '3', '1']);
        assert_eq!(generator.next(), vec!['2', '3', '2', '0']);
        assert_eq!(generator.next(), vec!['2', '3', '2', '1']);
        assert_eq!(generator.next(), vec!['2', '3', '3', '0']);
        assert_eq!(generator.next(), vec!['2', '3', '3', '1']);
        assert_eq!(generator.next(), vec!['3', '2', '2', '0']);
        assert_eq!(generator.next(), vec!['3', '2', '2', '1']);
        assert_eq!(generator.next(), vec!['3', '2', '3', '0']);
        assert_eq!(generator.next(), vec!['3', '2', '3', '1']);
        assert_eq!(generator.next(), vec!['3', '3', '2', '0']);
        assert_eq!(generator.next(), vec!['3', '3', '2', '1']);
        assert_eq!(generator.next(), vec!['3', '3', '3', '0']);
        assert_eq!(generator.next(), vec!['3', '3', '3', '1']);
    }

    #[test]
    fn closed_bracket_does_not_result_in_infinite_iterator() {
        let term = mock_term(" ) ");

        let mut search = RegexSearch::new("[^/ ]").unwrap();

        let count = HintPostProcessor::new(
            &term,
            &mut search,
            Point::new(Line(0), Column(1))..=Point::new(Line(0), Column(1)),
        )
        .take(1)
        .count();

        assert_eq!(count, 0);
    }

    #[test]
    fn collect_unique_hyperlinks() {
        let mut term = mock_term("000\r\n111");
        term.goto(0, 0);

        let hyperlink_foo = Hyperlink::new(Some("1"), String::from("foo"));
        let hyperlink_bar = Hyperlink::new(Some("2"), String::from("bar"));

        // Create 2 hyperlinks on the first line.
        term.set_hyperlink(Some(hyperlink_foo.clone().into()));
        term.input('b');
        term.input('a');
        term.set_hyperlink(Some(hyperlink_bar.clone().into()));
        term.input('r');
        term.set_hyperlink(Some(hyperlink_foo.clone().into()));
        term.goto(1, 0);

        // Ditto for the second line.
        term.set_hyperlink(Some(hyperlink_foo.into()));
        term.input('b');
        term.input('a');
        term.set_hyperlink(Some(hyperlink_bar.into()));
        term.input('r');
        term.set_hyperlink(None);

        let mut unique_hyperlinks = visible_unique_hyperlinks_iter(&term);
        assert_eq!(
            Some(Match::new(Point::new(Line(0), Column(0)), Point::new(Line(0), Column(1)))),
            unique_hyperlinks.next()
        );
        assert_eq!(
            Some(Match::new(Point::new(Line(0), Column(2)), Point::new(Line(0), Column(2)))),
            unique_hyperlinks.next()
        );
        assert_eq!(None, unique_hyperlinks.next());
    }

    #[test]
    fn visible_regex_match_covers_entire_viewport() {
        let content = "I'm a match!\r\n".repeat(4096);
        // The Term returned from this call will have a viewport starting at 0 and ending at 4096.
        // That's good enough for this test, since it only cares about visible content.
        let term = mock_term(&content);
        let mut regex = RegexSearch::new("match!").unwrap();

        // The iterator should match everything in the viewport.
        assert_eq!(visible_regex_match_iter(&term, &mut regex).count(), 4096);
    }
}
