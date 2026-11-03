//! Display of right-to-left text.
//!
//! The terminal grid stores text in logical order with one character per cell. Before rendering,
//! Arabic letters are replaced by the presentation form matching their joining context and every
//! line containing right-to-left text is reordered using the Unicode Bidirectional Algorithm.
//!
//! This only changes how cells are drawn, the resulting [`RtlLines`] layout is used to map mouse
//! positions back to the grid.

use std::collections::HashMap;

use alacritty_terminal::index::{Column, Point};
use alacritty_terminal::term::cell::Flags;
use unicode_bidi::{BidiClass, BidiInfo, Level};

use crate::display::content::{RenderableCell, RenderableCursor};

const TATWEEL: char = '\u{0640}';
const LAM: char = '\u{0644}';
const ZWNJ: char = '\u{200C}';

/// Arabic letters with their isolated presentation form and whether they join on both sides.
///
/// The final, initial, and medial forms directly follow the isolated form. Letters which only
/// join to the preceding letter just have an isolated and final form.
const ARABIC_FORMS: [(char, u32, bool); 41] = [
    ('\u{0622}', 0xFE81, false), // Alef with madda above.
    ('\u{0623}', 0xFE83, false), // Alef with hamza above.
    ('\u{0624}', 0xFE85, false), // Waw with hamza above.
    ('\u{0625}', 0xFE87, false), // Alef with hamza below.
    ('\u{0626}', 0xFE89, true),  // Yeh with hamza above.
    ('\u{0627}', 0xFE8D, false), // Alef.
    ('\u{0628}', 0xFE8F, true),  // Beh.
    ('\u{0629}', 0xFE93, false), // Teh marbuta.
    ('\u{062A}', 0xFE95, true),  // Teh.
    ('\u{062B}', 0xFE99, true),  // Theh.
    ('\u{062C}', 0xFE9D, true),  // Jeem.
    ('\u{062D}', 0xFEA1, true),  // Hah.
    ('\u{062E}', 0xFEA5, true),  // Khah.
    ('\u{062F}', 0xFEA9, false), // Dal.
    ('\u{0630}', 0xFEAB, false), // Thal.
    ('\u{0631}', 0xFEAD, false), // Reh.
    ('\u{0632}', 0xFEAF, false), // Zain.
    ('\u{0633}', 0xFEB1, true),  // Seen.
    ('\u{0634}', 0xFEB5, true),  // Sheen.
    ('\u{0635}', 0xFEB9, true),  // Sad.
    ('\u{0636}', 0xFEBD, true),  // Dad.
    ('\u{0637}', 0xFEC1, true),  // Tah.
    ('\u{0638}', 0xFEC5, true),  // Zah.
    ('\u{0639}', 0xFEC9, true),  // Ain.
    ('\u{063A}', 0xFECD, true),  // Ghain.
    ('\u{0641}', 0xFED1, true),  // Feh.
    ('\u{0642}', 0xFED5, true),  // Qaf.
    ('\u{0643}', 0xFED9, true),  // Kaf.
    ('\u{0644}', 0xFEDD, true),  // Lam.
    ('\u{0645}', 0xFEE1, true),  // Meem.
    ('\u{0646}', 0xFEE5, true),  // Noon.
    ('\u{0647}', 0xFEE9, true),  // Heh.
    ('\u{0648}', 0xFEED, false), // Waw.
    ('\u{0649}', 0xFEEF, false), // Alef maksura.
    ('\u{064A}', 0xFEF1, true),  // Yeh.
    ('\u{067E}', 0xFB56, true),  // Peh.
    ('\u{0686}', 0xFB7A, true),  // Tcheh.
    ('\u{0698}', 0xFB8A, false), // Jeh.
    ('\u{06A9}', 0xFB8E, true),  // Keheh.
    ('\u{06AF}', 0xFB92, true),  // Gaf.
    ('\u{06CC}', 0xFBFC, true),  // Farsi yeh.
];

/// Displayed layout of all reordered viewport lines.
#[derive(Debug, Default)]
pub struct RtlLines(HashMap<usize, Vec<(Column, bool)>>);

impl RtlLines {
    /// Grid column displayed at a viewport point and whether its text is right-to-left.
    pub fn logical(&self, point: Point<usize>) -> (Column, bool) {
        let column = self.0.get(&point.line).and_then(|columns| columns.get(point.column.0));
        column.copied().unwrap_or((point.column, false))
    }

    /// Viewport lines which were reordered.
    pub fn lines(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.keys().copied()
    }
}

/// Shape and reorder all lines containing right-to-left text.
///
/// The cells must be ordered by line, the cursor is moved together with the cell below it.
pub fn reorder(
    cells: &mut [RenderableCell],
    columns: usize,
    cursor: &mut RenderableCursor,
) -> RtlLines {
    let mut rtl_lines = RtlLines::default();

    for cells in cells.chunk_by_mut(|a, b| a.point.line == b.point.line) {
        if !cells.iter().any(|cell| is_rtl(cell.character)) {
            continue;
        }

        // Logical text of the entire line, since empty cells are not part of the renderable cells.
        let line = cells[0].point.line;
        let mut chars = vec![' '; columns];
        let mut non_joiners = vec![false; columns];
        for cell in cells.iter() {
            let column = cell.point.column.0;
            chars[column] = cell.character;
            if cell.flags.contains(Flags::WIDE_CHAR) {
                if let Some(spacer) = chars.get_mut(column + 1) {
                    *spacer = cell.character;
                }
            }

            // Zero-width non-joiners are stored in the cell before them.
            let zerowidth = cell.extra.as_ref().and_then(|extra| extra.zerowidth.as_ref());
            non_joiners[column] = zerowidth.is_some_and(|zerowidth| zerowidth.contains(&ZWNJ));
        }

        let text: String = chars.iter().collect();
        let bidi = BidiInfo::new(&text, Some(Level::ltr()));
        let levels = bidi.reordered_levels_per_char(&bidi.paragraphs[0], 0..text.len());

        let logical_columns = BidiInfo::reorder_visual(&levels);
        let mut visual_columns = vec![0; columns];
        for (visual, &logical) in logical_columns.iter().enumerate() {
            visual_columns[logical] = visual;
        }

        // Wide characters start at whichever of their two columns ends up further left.
        let visual_column = |column: usize, wide: bool| {
            let end = visual_columns.get(column + wide as usize).copied();
            Column(end.map_or(visual_columns[column], |end| end.min(visual_columns[column])))
        };

        for cell in cells.iter_mut() {
            let column = cell.point.column.0;
            let prev = match column.checked_sub(1) {
                Some(prev) if !non_joiners[prev] => chars[prev],
                _ => ' ',
            };
            let next = match chars.get(column + 1) {
                Some(&next) if !non_joiners[column] => next,
                _ => ' ',
            };

            cell.character = shape(prev, chars[column], next);
            if levels[column].is_rtl() {
                cell.character = mirror(cell.character);
            }

            cell.point.column = visual_column(column, cell.flags.contains(Flags::WIDE_CHAR));
        }

        // Underline and strikeout merging requires cells in visual order.
        cells.sort_unstable_by_key(|cell| cell.point.column);

        if cursor.point().line == line {
            let wide = cursor.width().get() == 2;
            let point = Point::new(line, visual_column(cursor.point().column.0, wide));
            *cursor = RenderableCursor::new(point, cursor.shape(), cursor.color(), cursor.width());
        }

        let layout =
            logical_columns.into_iter().map(|column| (Column(column), levels[column].is_rtl()));
        rtl_lines.0.insert(line, layout.collect());
    }

    rtl_lines
}

/// Check if a character is written right-to-left.
fn is_rtl(c: char) -> bool {
    c >= '\u{0590}' && matches!(unicode_bidi::bidi_class(c), BidiClass::R | BidiClass::AL)
}

/// Get the presentation form of a character based on its logical neighbors.
fn shape(prev: char, c: char, next: char) -> char {
    // Draw the lam-alef ligature in lam's cell, leaving alef's cell empty.
    if c == LAM {
        if let Some(ligature) = lam_alef(next) {
            return char::from_u32(ligature + joins_next(prev) as u32).unwrap_or(c);
        }
    } else if prev == LAM && lam_alef(c).is_some() {
        return ' ';
    }

    let Some((isolated, dual_joining)) = forms(c) else {
        return c;
    };

    let offset = match (joins_next(prev), dual_joining && joins_prev(next)) {
        (false, false) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (true, true) => 3,
    };

    char::from_u32(isolated + offset).unwrap_or(c)
}

/// Isolated form of the lam-alef ligature, followed by its final form.
fn lam_alef(alef: char) -> Option<u32> {
    match alef {
        '\u{0622}' => Some(0xFEF5),
        '\u{0623}' => Some(0xFEF7),
        '\u{0625}' => Some(0xFEF9),
        '\u{0627}' => Some(0xFEFB),
        _ => None,
    }
}

/// Isolated presentation form and dual-joining property of an Arabic letter.
fn forms(c: char) -> Option<(u32, bool)> {
    let index = ARABIC_FORMS.binary_search_by_key(&c, |&(base, ..)| base).ok()?;
    let (_, isolated, dual_joining) = ARABIC_FORMS[index];
    Some((isolated, dual_joining))
}

/// Check if a character connects to the logically following letter.
fn joins_next(c: char) -> bool {
    c == TATWEEL || forms(c).is_some_and(|(_, dual_joining)| dual_joining)
}

/// Check if a character connects to the logically preceding letter.
fn joins_prev(c: char) -> bool {
    c == TATWEEL || forms(c).is_some()
}

/// Mirror a character displayed right-to-left.
// ponytail: common brackets only, use the full Bidi_Mirroring_Glyph table if others show up.
fn mirror(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use alacritty_terminal::vte::ansi::CursorShape;

    use super::*;
    use crate::display::color::Rgb;
    use crate::display::content::RenderableCellExtra;

    /// Reorder a single line, returning its visual text, cursor column, and layout.
    fn render(text: &str, cursor_column: usize) -> (String, usize, RtlLines) {
        let columns = 12;
        let mut cells: Vec<RenderableCell> = Vec::new();
        let mut column = 0;
        for character in text.chars() {
            if character == ZWNJ {
                let extra = RenderableCellExtra { zerowidth: Some(vec![ZWNJ]), hyperlink: None };
                cells.last_mut().unwrap().extra = Some(Box::new(extra));
                continue;
            } else if character != ' ' {
                cells.push(RenderableCell {
                    character,
                    point: Point::new(0, Column(column)),
                    fg: Rgb::default(),
                    bg: Rgb::default(),
                    bg_alpha: 0.,
                    underline: Rgb::default(),
                    flags: Flags::empty(),
                    extra: None,
                });
            }
            column += 1;
        }
        let point = Point::new(0, Column(cursor_column));
        let width = NonZeroU32::new(1).unwrap();
        let mut cursor = RenderableCursor::new(point, CursorShape::Beam, Rgb::default(), width);

        let rtl_lines = reorder(&mut cells, columns, &mut cursor);
        assert_eq!(rtl_lines.lines().next().is_none(), !text.chars().any(is_rtl));
        assert!(cells.is_sorted_by_key(|cell| cell.point.column));

        let mut line = vec![' '; columns];
        for cell in cells {
            line[cell.point.column.0] = cell.character;
        }
        let line = line.into_iter().collect::<String>().trim_end().into();
        (line, cursor.point().column.0, rtl_lines)
    }

    /// Visual text and cursor column of a single line.
    fn display(text: &str, cursor_column: usize) -> (String, usize) {
        let (line, cursor_column, _) = render(text, cursor_column);
        (line, cursor_column)
    }

    #[test]
    fn forms_sorted() {
        assert!(ARABIC_FORMS.is_sorted());
    }

    #[test]
    fn ltr_untouched() {
        assert_eq!(display("echo (1)", 3), ("echo (1)".into(), 3));
    }

    #[test]
    fn joined_and_reversed() {
        // Seen (initial), lam-alef (final), meem (isolated).
        assert_eq!(display("سلام", 0), ("\u{FEE1} \u{FEFC}\u{FEB3}".into(), 3));

        // Beh (initial), teh (medial), heh (final).
        assert_eq!(display("$ بته", 5), ("$ \u{FEEA}\u{FE98}\u{FE91}".into(), 5));
    }

    #[test]
    fn numbers_and_brackets() {
        // Numbers stay left-to-right and brackets are mirrored within right-to-left text.
        assert_eq!(display("ب (12)", 0), ("(12) \u{FE8F}".into(), 5));
    }

    #[test]
    fn zero_width_non_joiner() {
        // Meem (initial), yeh (final), khah (isolated), since the non-joiner breaks the word.
        assert_eq!(display("می\u{200C}خ", 3), ("\u{FEA5}\u{FBFD}\u{FEE3}".into(), 3));
    }

    #[test]
    fn mouse_mapping() {
        let (_, _, rtl_lines) = render("ab بت", 0);
        let logical = |column| rtl_lines.logical(Point::new(0, Column(column)));

        assert_eq!(logical(0), (Column(0), false));
        assert_eq!(logical(3), (Column(4), true));
        assert_eq!(logical(4), (Column(3), true));
        assert_eq!(logical(8), (Column(8), false));

        // Lines without right-to-left text are not modified.
        assert_eq!(rtl_lines.logical(Point::new(1, Column(3))), (Column(3), false));
    }
}
