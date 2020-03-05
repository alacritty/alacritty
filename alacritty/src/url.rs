use std::cmp::min;
use std::ops::RangeInclusive;

use urlocator::{UrlLocation, UrlLocator};

use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::Term;

/// Maximum number of lines checked for URL parsing.
const MAX_URL_LINES: usize = 100;

// TODO: Do we need url.rs?
//  -> Maybe something more like search.rs?
pub fn url_at_point<T>(term: &Term<T>, point: Point) -> Option<RangeInclusive<Point<usize>>> {
    let num_cols = term.grid().num_cols();

    let buffer_point = term.visible_to_buffer(point);
    let mut first_line = term.grid().wrapped_line_start(Line(buffer_point.line)).0;
    first_line = min(buffer_point.line + MAX_URL_LINES, first_line);
    let search_start = Point::new(first_line, Column(0));

    let mut iter = term.grid().iter_from(search_start);

    let mut locator = UrlLocator::new();
    locator.advance(iter.cell().c);
    let mut url = None;

    while let Some(cell) = iter.next() {
        let point = iter.point();

        match (locator.advance(cell.c), url) {
            (UrlLocation::Url(length, end_offset), _) => {
                let end = point.sub_absolute(num_cols.0, end_offset as usize);
                let start = end.sub_absolute(num_cols.0, length as usize - 1);
                url = Some((start, end));
            },
            (UrlLocation::Reset, Some((start, end))) => {
                // Skip URLs unless we've reached the original point
                if end.line < buffer_point.line
                    || (end.line == buffer_point.line && end.col >= buffer_point.col)
                {
                    // Check if the original point is within the URL
                    if start.line > buffer_point.line
                        || (start.line == buffer_point.line && start.col <= buffer_point.col)
                    {
                        return Some(start..=end);
                    } else {
                        return None;
                    }
                }
            },
            _ => (),
        }

        // Reached the end of the line
        if point.col + 1 == num_cols && !cell.flags.contains(Flags::WRAPLINE) {
            break;
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::test::mock_term;

    #[test]
    fn no_url() {
        #[rustfmt::skip]
        let term = mock_term("\
            testing a long thing without URL\n\
            huh https://example.org 134\n\
            test short\
        ");

        assert_eq!(url_at_point(&term, Point::new(Line(2), Column(22))), None);
        assert_eq!(url_at_point(&term, Point::new(Line(1), Column(3))), None);
        assert_eq!(url_at_point(&term, Point::new(Line(1), Column(23))), None);
        assert_eq!(url_at_point(&term, Point::new(Line(0), Column(4))), None);
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
        assert_eq!(url_at_point(&term, start), Some(start..=end));
        assert_eq!(url_at_point(&term, end), Some(start..=end));

        let start = Point::new(1, Column(26));
        let end = Point::new(1, Column(46));
        assert_eq!(url_at_point(&term, start), Some(start..=end));
        assert_eq!(url_at_point(&term, end), Some(start..=end));
    }
}
