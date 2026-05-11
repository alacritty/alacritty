//! Detect clickable links in the terminal grid.
//!
//! Mirrors alacritty's default URL hint behaviour: OSC 8 hyperlinks and
//! `scheme://...` URLs visible in the viewport are recognised so the
//! terminal view can underline them on hover and open them on click.

use std::cell::RefCell;
use std::process::{Command, Stdio};

use alacritty_terminal::grid::BidirectionalIterator;
use alacritty_terminal::index::{Direction, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};

use crate::session::EventProxy;

// Identical to alacritty's built-in URL hint regex so the set of recognised
// schemes (and the trailing-character rules) stay in sync with what users
// experience in vanilla alacritty.
#[rustfmt::skip]
const URL_REGEX: &str = "(ipfs:|ipns:|magnet:|mailto:|gemini://|gopher://|https://|http://|news:|file:|git://|ssh:|ftp://)\
                         [^\u{0000}-\u{001F}\u{007F}-\u{009F}<>\"\\s{-}\\^⟨⟩`\\\\]+";

thread_local! {
    // RegexSearch holds lazily-populated DFAs and needs `&mut` to query, but
    // recompiling the pattern every frame is wasteful.  egui runs the UI on
    // one thread, so a TLS RefCell is enough.
    static URL_SEARCH: RefCell<RegexSearch> =
        RefCell::new(RegexSearch::new(URL_REGEX).expect("URL_REGEX must compile"));
}

#[derive(Debug, Clone)]
pub struct Link {
    pub bounds: Match,
    pub uri: String,
}

/// Find a clickable link covering `point`, if any.
///
/// OSC 8 hyperlinks take priority over regex matches because they carry an
/// explicit URI that may differ from the visible text.
pub fn link_at(term: &Term<EventProxy>, point: Point) -> Option<Link> {
    if let Some(link) = hyperlink_at(term, point) {
        return Some(link);
    }

    URL_SEARCH.with(|cell| {
        let mut regex = cell.borrow_mut();
        let bounds = url_match_at(term, point, &mut regex)?;
        let uri = term.bounds_to_string(*bounds.start(), *bounds.end());
        Some(Link { bounds, uri })
    })
}

fn hyperlink_at(term: &Term<EventProxy>, point: Point) -> Option<Link> {
    let hyperlink = term.grid()[point].hyperlink()?;
    let grid = term.grid();

    let mut end = point;
    for cell in grid.iter_from(point) {
        if cell.hyperlink().is_some_and(|h| h == hyperlink) {
            end = cell.point;
        } else {
            break;
        }
    }

    let mut start = point;
    let mut iter = grid.iter_from(point);
    while let Some(cell) = iter.prev() {
        if cell.hyperlink().is_some_and(|h| h == hyperlink) {
            start = cell.point;
        } else {
            break;
        }
    }

    Some(Link { bounds: start..=end, uri: hyperlink.uri().to_owned() })
}

fn url_match_at(term: &Term<EventProxy>, point: Point, regex: &mut RegexSearch) -> Option<Match> {
    // URLs can wrap, so the scan range is the full logical line that contains
    // `point` — `line_search_left/right` follow WRAPLINE flags to cover that.
    let line_start = term.line_search_left(point);
    let line_end = term.line_search_right(point);

    let regex_match = RegexIter::new(line_start, line_end, Direction::Right, term, regex)
        .find(|rm| rm.contains(&point))?;

    post_process(term, regex_match, point)
}

/// Strip trailing punctuation and unbalanced brackets that the regex greedily
/// includes.  Same heuristic alacritty's `HintPostProcessor` uses so a URL
/// embedded in prose (`see (https://example.com).`) opens at the right bound.
fn post_process(term: &Term<EventProxy>, regex_match: Match, point: Point) -> Option<Match> {
    let mut iter = term.grid().iter_from(*regex_match.start());
    let end = *regex_match.end();
    let mut c = iter.cell().c;

    let mut open_parens = 0i32;
    let mut open_brackets = 0i32;
    loop {
        match c {
            '(' => open_parens += 1,
            '[' => open_brackets += 1,
            ')' => {
                if open_parens == 0 {
                    iter.prev();
                    break;
                }
                open_parens -= 1;
            },
            ']' => {
                if open_brackets == 0 {
                    iter.prev();
                    break;
                }
                open_brackets -= 1;
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

    if start > iter.point() {
        return None;
    }
    let trimmed = start..=iter.point();
    trimmed.contains(&point).then_some(trimmed)
}

/// Hand the URI to the OS handler — `xdg-open` on Linux/BSDs, `open` on macOS,
/// `cmd /c start` on Windows — matching alacritty's default URL hint action.
pub fn open(uri: &str) {
    let result = spawn(uri);
    if let Err(err) = result {
        log::warn!("failed to open link {uri:?}: {err}");
    }
}

#[cfg(target_os = "macos")]
fn spawn(uri: &str) -> std::io::Result<()> {
    Command::new("open")
        .arg(uri)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "windows")]
fn spawn(uri: &str) -> std::io::Result<()> {
    // `start` treats its first quoted argument as a window title, so pass an
    // empty title before the URL to keep `cmd` from eating it.
    Command::new("cmd")
        .args(["/c", "start", ""])
        .arg(uri)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn spawn(uri: &str) -> std::io::Result<()> {
    Command::new("xdg-open")
        .arg(uri)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}
