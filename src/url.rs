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

use url::Url;

// See https://tools.ietf.org/html/rfc3987#page-13
const URL_SEPARATOR_CHARS: [char; 10] = ['<', '>', '"', ' ', '{', '}', '|', '\\', '^', '`'];
const URL_DENY_END_CHARS: [char; 8] = ['.', ',', ';', ':', '?', '!', '/', '('];
const URL_SCHEMES: [&str; 8] = [
    "http", "https", "mailto", "news", "file", "git", "ssh", "ftp",
];

// Parser for streaming inside-out detection of URLs.
pub struct UrlParser {
    state: String,
}

impl UrlParser {
    pub fn new() -> Self {
        UrlParser {
            state: String::new(),
        }
    }

    /// Advance the parser one character to the left.
    pub fn advance_left(&mut self, c: char) -> bool {
        self.advance(c, 0)
    }

    /// Advance the parser one character to the right.
    pub fn advance_right(&mut self, c: char) -> bool {
        self.advance(c, self.state.len())
    }

    /// Returns the URL if the parser has found any.
    pub fn url(mut self) -> Option<String> {
        // Remove non-alphabetical characters before the scheme
        // https://tools.ietf.org/html/rfc3986#section-3.1
        if let Some(index) = self.state.find("://") {
            let iter = self
                .state
                .char_indices()
                .rev()
                .skip_while(|(byte_index, _)| byte_index >= &index);
            for (byte_index, c) in iter {
                match c {
                    'a'...'z' | 'A'...'Z' => (),
                    _ => {
                        self.state = self.state.split_off(byte_index + c.len_utf8());
                        break;
                    }
                }
            }
        }

        // Remove non-matching parenthesis and brackets
        let mut open_parens_count: isize = 0;
        let mut open_bracks_count: isize = 0;
        for (i, c) in self.state.chars().enumerate() {
            match c {
                '(' => open_parens_count += 1,
                ')' if open_parens_count > 0 => open_parens_count -= 1,
                '[' => open_bracks_count += 1,
                ']' if open_bracks_count > 0 => open_bracks_count -= 1,
                ')' | ']' => {
                    self.state.truncate(i);
                    break;
                }
                _ => (),
            }
        }

        // Track number of quotes
        let mut num_quotes = self.state.chars().filter(|&c| c == '\'').count();

        // Remove all characters which aren't allowed at the end of a URL
        while !self.state.is_empty()
            && (URL_DENY_END_CHARS.contains(&self.state.chars().last().unwrap())
                || (num_quotes % 2 != 0 && self.state.ends_with('\''))
                || self.state.ends_with("''")
                || self.state.ends_with("()"))
        {
            if self.state.pop().unwrap() == '\'' {
                num_quotes -= 1;
            }
        }

        // Check if string is valid url
        match Url::parse(&self.state) {
            Ok(url) => {
                if URL_SCHEMES.contains(&url.scheme()) {
                    Some(self.state)
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    }

    fn advance(&mut self, c: char, pos: usize) -> bool {
        if URL_SEPARATOR_CHARS.contains(&c)
            || (c >= '\u{00}' && c <= '\u{1F}')
            || (c >= '\u{7F}' && c <= '\u{9F}')
        {
            true
        } else {
            self.state.insert(pos, c);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::mem;

    use crate::grid::Grid;
    use crate::index::{Column, Line, Point};
    use crate::term::{Cell, Search, SizeInfo, Term};

    fn url_create_term(input: &str) -> Term {
        let size = SizeInfo {
            width: 21.0,
            height: 51.0,
            cell_width: 3.0,
            cell_height: 3.0,
            padding_x: 0.0,
            padding_y: 0.0,
            dpr: 1.0,
        };

        let mut term = Term::new(&Default::default(), size);
        let mut grid: Grid<Cell> = Grid::new(Line(1), Column(input.len()), 0, Cell::default());

        for (i, c) in input.chars().enumerate() {
            grid[Line(0)][Column(i)].c = c;
        }

        mem::swap(term.grid_mut(), &mut grid);

        term
    }

    fn url_test(input: &str, expected: &str, click_index: usize) {
        let term = url_create_term(input);

        let url = term.url_search(Point::new(0, Column(click_index)));

        assert_eq!(url, Some(expected.into()));
    }

    #[test]
    fn url_skip_invalid() {
        let term = url_create_term("no url here");
        let url = term.url_search(Point::new(0, Column(4)));
        assert_eq!(url, None);
    }

    #[test]
    fn url_matching_chars() {
        url_test("(https://example.org/test(ing))", "https://example.org/test(ing)", 5);
        url_test("https://example.org/test(ing)", "https://example.org/test(ing)", 5);
        url_test("((https://example.org))", "https://example.org", 5);
        url_test(")https://example.org(", "https://example.org", 5);
        url_test("https://example.org)", "https://example.org", 5);
        url_test("https://example.org(", "https://example.org", 5);
        url_test("(https://one.org/)(https://two.org/)", "https://one.org", 5);

        url_test("https://[2001:db8:a0b:12f0::1]:80", "https://[2001:db8:a0b:12f0::1]:80", 5);
        url_test("([(https://example.org/test(ing))])", "https://example.org/test(ing)", 5);
        url_test("https://example.org/]()", "https://example.org", 5);
        url_test("[https://example.org]", "https://example.org", 5);

        url_test("'https://example.org/test'ing'''", "https://example.org/test'ing'", 5);
        url_test("https://example.org/test'ing'", "https://example.org/test'ing'", 5);
        url_test("'https://example.org'", "https://example.org", 5);
        url_test("'https://example.org", "https://example.org", 5);
        url_test("https://example.org'", "https://example.org", 5);
    }

    #[test]
    fn url_detect_end() {
        url_test("https://example.org/test\u{00}ing", "https://example.org/test", 5);
        url_test("https://example.org/test\u{1F}ing", "https://example.org/test", 5);
        url_test("https://example.org/test\u{7F}ing", "https://example.org/test", 5);
        url_test("https://example.org/test\u{9F}ing", "https://example.org/test", 5);
        url_test("https://example.org/test\ting", "https://example.org/test", 5);
        url_test("https://example.org/test ing", "https://example.org/test", 5);
    }

    #[test]
    fn url_remove_end_chars() {
        url_test("https://example.org/test?ing", "https://example.org/test?ing", 5);
        url_test("https://example.org.,;:)'!/?", "https://example.org", 5);
        url_test("https://example.org'.", "https://example.org", 5);
    }

    #[test]
    fn url_remove_start_chars() {
        url_test("complicated:https://example.org", "https://example.org", 15);
        url_test("test.https://example.org", "https://example.org", 10);
        url_test(",https://example.org", "https://example.org", 5);
        url_test("\u{2502}https://example.org", "https://example.org", 5);
    }

    #[test]
    fn url_unicode() {
        url_test("https://xn--example-2b07f.org", "https://xn--example-2b07f.org", 5);
        url_test("https://example.org/\u{2008A}", "https://example.org/\u{2008A}", 5);
        url_test("https://example.org/\u{f17c}", "https://example.org/\u{f17c}", 5);
        url_test("https://üñîçøðé.com/ä", "https://üñîçøðé.com/ä", 5);
    }

    #[test]
    fn url_schemes() {
        url_test("mailto://example.org", "mailto://example.org", 5);
        url_test("https://example.org", "https://example.org", 5);
        url_test("http://example.org", "http://example.org", 5);
        url_test("news://example.org", "news://example.org", 5);
        url_test("file://example.org", "file://example.org", 5);
        url_test("git://example.org", "git://example.org", 5);
        url_test("ssh://example.org", "ssh://example.org", 5);
        url_test("ftp://example.org", "ftp://example.org", 5);
    }
}
