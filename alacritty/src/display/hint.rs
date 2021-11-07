use std::cmp::{max, min, Reverse};
use std::collections::HashSet;

use glutin::event::ModifiersState;

use alacritty_terminal::grid::{BidirectionalIterator, Dimensions};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point};
use alacritty_terminal::term::hyperlink::Hyperlink;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::term::{Term, TermMode};

use crate::config::ui_config::{Hint, HintAction};
use crate::config::Config;
use crate::display::content::RegexMatches;

/// Maximum number of linewraps followed outside of the viewport during search highlighting.
pub const MAX_SEARCH_LINES: usize = 100;

/// Percentage of characters in the hints alphabet used for the last character.
const HINT_SPLIT_PERCENTAGE: f32 = 0.5;

/// Keyboard hint state.
pub struct HintState {
    /// Hint currently in use.
    hint: Option<Hint>,

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
    pub fn start(&mut self, hint: Hint) {
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

        let mut hint_matches: Vec<Match> = Vec::new();

        // Find hyperlinks.
        if hint.hyperlinks {
            let mut visited = HashSet::new();
            for (hyperlink, bounds) in visible_hyperlink_iter(term) {
                // Only make the first part of a hyperlink is navigate-able, if there are many.
                if visited.insert(hyperlink) {
                    hint_matches.push(bounds);
                }
            }
        }

        // Find regex matches.
        if let Some(regex) = &hint.regex {
            regex.with_compiled(|regex| {
                let matches = RegexMatches::new(term, regex);

                // Apply post-processing and search for sub-matches if necessary.
                if hint.post_processing {
                    hint_matches.extend(
                        matches
                            .0
                            .into_iter()
                            .map(|rm| HintPostProcessor::new(term, regex, rm).collect::<Vec<_>>())
                            .flatten(),
                    );
                } else {
                    hint_matches.extend(matches.0);
                }
            });
        }

        // Sort and dedup ranges. Currently overlapped but not exactly same ranges are kept.
        hint_matches.sort_by_key(|bounds| (*bounds.start(), Reverse(*bounds.end())));
        hint_matches.dedup_by_key(|bounds| *bounds.start());

        self.matches = hint_matches;

        // Cancel highlight with no visible matches.
        if self.matches.is_empty() {
            self.stop();
            return;
        }

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
            let action = hint.action.clone();

            self.stop();

            match term.hyperlink_at(*bounds.start()) {
                // Hyperlinks take precedence over regex matches.
                Some(hyperlink) => Some(HintMatch::Hyperlink { hyperlink, action, bounds }),
                None => Some(HintMatch::Regex { action, bounds }),
            }
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

    /// Visible hint matches.
    pub fn matches(&self) -> &[Match] {
        &self.matches
    }

    /// Update the alphabet used for hint labels.
    pub fn update_alphabet(&mut self, alphabet: &str) {
        if self.alphabet != alphabet {
            self.alphabet = alphabet.to_owned();
            self.keys.clear();
        }
    }
}

/// Hint match which was selected by the user.
#[derive(PartialEq, Debug)]
pub enum HintMatch {
    Regex {
        /// Action for handling the text.
        action: HintAction,
        /// Terminal range matching the hint.
        bounds: Match,
    },
    Hyperlink {
        /// Action for handling the text.
        action: HintAction,
        hyperlink: Hyperlink,
        bounds: Match,
    },
}

impl HintMatch {
    #[inline]
    pub fn should_highlight(&self, point: Point, pointed_hyperlink: Option<&Hyperlink>) -> bool {
        match self {
            Self::Regex { bounds, .. } => bounds.contains(&point),
            Self::Hyperlink { hyperlink, .. } => Some(hyperlink) == pointed_hyperlink,
        }
    }

    pub fn action(&self) -> &HintAction {
        match self {
            Self::Regex { action, .. } | Self::Hyperlink { action, .. } => action,
        }
    }

    pub fn bounds(&self) -> Match {
        match self {
            Self::Regex { bounds, .. } | Self::Hyperlink { bounds, .. } => bounds.clone(),
        }
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

/// Iterate all visible hyperlinks.
/// Multiple adjacent cells from the same hyperlink are joined into a single range.
fn visible_hyperlink_iter<T>(term: &Term<T>) -> impl Iterator<Item = (Hyperlink, Match)> + '_ {
    let mut grid_iter = term
        .grid()
        .display_iter()
        // Keep None here. It should stop the `while` below since non-links should cut links.
        .map(|cell| Some((cell.point, cell.hyperlink().cloned()?)))
        .peekable();

    std::iter::from_fn(move || {
        // Find the next hyperlink.
        let (start_point, hyperlink) = grid_iter.find_map(|cell| cell)?;
        let mut end_point = start_point;
        // Extend until another link or a non-link cell.
        loop {
            match grid_iter.peek() {
                Some(Some((next_point, next_hyperlink))) if &hyperlink == next_hyperlink => {
                    end_point = *next_point;
                    let _ = grid_iter.next();
                },
                _ => break,
            }
        }
        Some((hyperlink, start_point..=end_point))
    })
}

/// Check if there is a hint highlighted at the specified point.
pub fn highlighted_at<T>(
    term: &Term<T>,
    config: &Config,
    point: Point,
    mouse_mods: ModifiersState,
) -> Option<HintMatch> {
    let mouse_mode = term.mode().intersects(TermMode::MOUSE_MODE);

    config.ui_config.hints.enabled.iter().find_map(|hint| {
        // Check if all required modifiers are pressed.
        let highlight = hint.mouse.map_or(false, |mouse| {
            mouse.enabled
                && mouse_mods.contains(mouse.mods.0)
                && (!mouse_mode || mouse_mods.contains(ModifiersState::SHIFT))
        });
        if !highlight {
            return None;
        }

        // TODO: Use `bool::then` instead of double-if when MSRV >= 1.50.0
        if hint.hyperlinks {
            if let Some((hyperlink, bounds)) = hyperlink_at(term, point) {
                return Some(HintMatch::Hyperlink {
                    hyperlink,
                    bounds,
                    action: hint.action.clone(),
                });
            }
        }

        if let Some(bounds) = hint.regex.as_ref().and_then(|regex| {
            regex.with_compiled(|regex| regex_match_at(term, point, regex, hint.post_processing))
        }) {
            return Some(HintMatch::Regex { bounds, action: hint.action.clone() });
        }

        None
    })
}

/// Retrive the hyperlink with its range, if there is one at the specified point.
fn hyperlink_at<T>(term: &Term<T>, point: Point) -> Option<(Hyperlink, Match)> {
    if term.hyperlink_at(point).is_none() {
        return None;
    }
    visible_hyperlink_iter(term).find(|(_, bounds)| bounds.contains(&point))
}

/// Retrive the match, if the specified point in inside the content matching the regex.
fn regex_match_at<T>(
    term: &Term<T>,
    point: Point,
    regex: &RegexSearch,
    post_processing: bool,
) -> Option<Match> {
    let viewport_start = Line(-(term.grid().display_offset() as i32));
    let viewport_end = viewport_start + term.bottommost_line();

    // Compute start of the first and end of the last line.
    let start_point = Point::new(viewport_start, Column(0));
    let mut start = term.line_search_left(start_point);
    let end_point = Point::new(viewport_end, term.last_column());
    let mut end = term.line_search_right(end_point);

    // Set upper bound on search before/after the viewport to prevent excessive blocking.
    start.line = max(start.line, viewport_start - MAX_SEARCH_LINES);
    end.line = min(end.line, viewport_end + MAX_SEARCH_LINES);

    // Function to verify that the specified point is inside the match.
    let at_point = |rm: &Match| *rm.end() >= point && *rm.start() <= point;

    // Check if there's any match at the specified point.
    let mut iter = RegexIter::new(start, end, Direction::Right, term, regex);
    let regex_match = iter.find(at_point)?;

    // Apply post-processing and search for sub-matches if necessary.
    if post_processing {
        HintPostProcessor::new(term, regex, regex_match).find(at_point)
    } else {
        Some(regex_match)
    }
}

/// Iterator over all post-processed matches inside an existing hint match.
struct HintPostProcessor<'a, T> {
    /// Regex search DFAs.
    regex: &'a RegexSearch,

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
    fn new(term: &'a Term<T>, regex: &'a RegexSearch, regex_match: Match) -> Self {
        let end = *regex_match.end();
        let mut post_processor = Self { next_match: None, start: end, end, term, regex };

        // Post-process the first hint match.
        let next_match = post_processor.hint_post_processing(&regex_match);
        post_processor.start = next_match.end().add(term, Boundary::Grid, 1);
        post_processor.next_match = Some(next_match);

        post_processor
    }

    /// Apply some hint post processing heuristics.
    ///
    /// This will check the end of the hint and make it shorter if certain characters are determined
    /// to be unlikely to be intentionally part of the hint.
    ///
    /// This is most useful for identifying URLs appropriately.
    fn hint_post_processing(&self, regex_match: &Match) -> Match {
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

        start..=iter.point()
    }
}

impl<'a, T> Iterator for HintPostProcessor<'a, T> {
    type Item = Match;

    fn next(&mut self) -> Option<Self::Item> {
        let next_match = self.next_match.take()?;

        if self.start <= self.end {
            if let Some(rm) = self.term.regex_search_right(self.regex, self.start, self.end) {
                let regex_match = self.hint_post_processing(&rm);
                self.start = regex_match.end().add(self.term, Boundary::Grid, 1);
                self.next_match = Some(regex_match);
            }
        }

        Some(next_match)
    }
}

#[cfg(test)]
mod tests {
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
}
