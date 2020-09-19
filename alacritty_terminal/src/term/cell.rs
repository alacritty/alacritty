use std::mem::ManuallyDrop;

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

use crate::ansi::{Color, NamedColor};
use crate::grid::{self, GridCell};
use crate::index::Column;

bitflags! {
    #[derive(Serialize, Deserialize)]
    pub struct Flags: u16 {
        const INVERSE                   = 0b0000_0000_0000_0001;
        const BOLD                      = 0b0000_0000_0000_0010;
        const ITALIC                    = 0b0000_0000_0000_0100;
        const BOLD_ITALIC               = 0b0000_0000_0000_0110;
        const UNDERLINE                 = 0b0000_0000_0000_1000;
        const WRAPLINE                  = 0b0000_0000_0001_0000;
        const WIDE_CHAR                 = 0b0000_0000_0010_0000;
        const WIDE_CHAR_SPACER          = 0b0000_0000_0100_0000;
        const DIM                       = 0b0000_0000_1000_0000;
        const DIM_BOLD                  = 0b0000_0000_1000_0010;
        const HIDDEN                    = 0b0000_0001_0000_0000;
        const STRIKEOUT                 = 0b0000_0010_0000_0000;
        const LEADING_WIDE_CHAR_SPACER  = 0b0000_0100_0000_0000;
        const DOUBLE_UNDERLINE          = 0b0000_1000_0000_0000;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
    #[serde(skip)]
    pub zerowidth: ManuallyDrop<Vec<char>>,
}

impl Default for Cell {
    fn default() -> Cell {
        Cell::new(' ', Color::Named(NamedColor::Foreground), Color::Named(NamedColor::Background))
    }
}

impl Drop for Cell {
    fn drop(&mut self) {
        // We wrap the zerowidth vector with manually drop here because STD's implementation is
        // slow for vectors that are usually empty. By doing this we can effectively avoid the slow
        // drop implementation unless necessary.
        if !self.zerowidth.is_empty() {
            unsafe {
                ManuallyDrop::drop(&mut self.zerowidth);
            }
        }
    }
}

impl GridCell for Cell {
    #[inline]
    fn is_empty(&self) -> bool {
        (self.c == ' ' || self.c == '\t')
            && self.zerowidth.is_empty()
            && self.bg == Color::Named(NamedColor::Background)
            && self.fg == Color::Named(NamedColor::Foreground)
            && !self.flags.intersects(
                Flags::INVERSE
                    | Flags::UNDERLINE
                    | Flags::DOUBLE_UNDERLINE
                    | Flags::STRIKEOUT
                    | Flags::WRAPLINE
                    | Flags::WIDE_CHAR_SPACER
                    | Flags::LEADING_WIDE_CHAR_SPACER,
            )
    }

    #[inline]
    fn flags(&self) -> &Flags {
        &self.flags
    }

    #[inline]
    fn flags_mut(&mut self) -> &mut Flags {
        &mut self.flags
    }

    // TODO: This method doesn't really make sense.
    #[inline]
    fn background(&self) -> Color {
        self.bg
    }

    #[inline]
    fn shallow_clone(&self) -> Self {
        Self { c: ' ', zerowidth: ManuallyDrop::new(Vec::new()), ..*self }
    }
}

/// Get the length of occupied cells in a line.
pub trait LineLength {
    /// Calculate the occupied line length.
    fn line_length(&self) -> Column;
}

impl LineLength for grid::Row<Cell> {
    fn line_length(&self) -> Column {
        let mut length = Column(0);

        if self[Column(self.len() - 1)].flags.contains(Flags::WRAPLINE) {
            return Column(self.len());
        }

        for (index, cell) in self[..].iter().rev().enumerate() {
            if cell.c != ' ' || !cell.zerowidth.is_empty() {
                length = Column(self.len() - index);
                break;
            }
        }

        length
    }
}

impl From<Color> for Cell {
    #[inline]
    fn from(color: Color) -> Self {
        Self {
            bg: color,
            c: ' ',
            flags: Flags::empty(),
            fg: Color::Named(NamedColor::Foreground),
            zerowidth: ManuallyDrop::new(Vec::new()),
        }
    }
}

impl Cell {
    #[inline]
    pub fn new(c: char, fg: Color, bg: Color) -> Cell {
        Cell { c, bg, fg, flags: Flags::empty(), zerowidth: ManuallyDrop::new(Vec::new()) }
    }

    #[inline]
    pub fn bold(&self) -> bool {
        self.flags.contains(Flags::BOLD)
    }

    #[inline]
    pub fn inverse(&self) -> bool {
        self.flags.contains(Flags::INVERSE)
    }

    #[inline]
    pub fn dim(&self) -> bool {
        self.flags.contains(Flags::DIM)
    }

    #[inline]
    pub fn reset(&mut self, bg: Color) {
        *self = bg.into();
    }

    #[inline]
    pub fn zerowidth(&self) -> &[char] {
        &self.zerowidth
    }

    #[inline]
    pub fn push_zerowidth(&mut self, c: char) {
        self.zerowidth.push(c);
    }
}

#[cfg(test)]
mod tests {
    use super::{Cell, LineLength};

    use crate::grid::Row;
    use crate::index::Column;

    #[test]
    fn line_length_works() {
        let template = Cell::default();
        let mut row = Row::new(Column(10), template);
        row[Column(5)].c = 'a';

        assert_eq!(row.line_length(), Column(6));
    }

    #[test]
    fn line_length_works_with_wrapline() {
        let template = Cell::default();
        let mut row = Row::new(Column(10), template);
        row[Column(9)].flags.insert(super::Flags::WRAPLINE);

        assert_eq!(row.line_length(), Column(10));
    }
}

#[cfg(all(test, feature = "bench"))]
mod benches {
    extern crate test;

    use super::*;

    #[bench]
    fn cell_reset(b: &mut test::Bencher) {
        b.iter(|| {
            let mut cell = Cell::default();

            for _ in 0..100 {
                cell.reset(test::black_box(Color::Named(NamedColor::Foreground)));
            }

            test::black_box(cell);
        });
    }
}
