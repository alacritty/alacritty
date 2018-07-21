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
use ansi::{NamedColor, Color};
use grid;
use index::Column;

bitflags! {
    #[derive(Serialize, Deserialize)]
    pub struct Flags: u32 {
        const INVERSE           = 0b0000_0001;
        const BOLD              = 0b0000_0010;
        const ITALIC            = 0b0000_0100;
        const UNDERLINE         = 0b0000_1000;
        const WRAPLINE          = 0b0001_0000;
        const WIDE_CHAR         = 0b0010_0000;
        const WIDE_CHAR_SPACER  = 0b0100_0000;
        const DIM               = 0b1000_0000;
        const DIM_BOLD          = 0b1000_0010;
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
}

impl Default for Cell {
    fn default() -> Cell {
        Cell::new(
            ' ',
            Color::Named(NamedColor::Foreground),
            Color::Named(NamedColor::Background)
        )
    }

}

/// Get the length of occupied cells in a line
pub trait LineLength {
    /// Calculate the occupied line length
    fn line_length(&self) -> Column;
}

impl LineLength for grid::Row<Cell> {
    fn line_length(&self) -> Column {
        let mut length = Column(0);

        if self[Column(self.len() - 1)].flags.contains(Flags::WRAPLINE) {
            return Column(self.len());
        }

        for (index, cell) in self[..].iter().rev().enumerate() {
            if cell.c != ' ' {
                length = Column(self.len() - index);
                break;
            }
        }

        length
    }
}

impl Cell {
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

    pub fn new(c: char, fg: Color, bg: Color) -> Cell {
        Cell {
            c,
            bg,
            fg,
            flags: Flags::empty(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.c == ' ' &&
            self.bg == Color::Named(NamedColor::Background) &&
            !self.flags.intersects(Flags::INVERSE | Flags::UNDERLINE)
    }

    #[inline]
    pub fn reset(&mut self, template: &Cell) {
        // memcpy template to self
        *self = *template;
    }
}

#[cfg(test)]
mod tests {
    use super::{Cell, LineLength};

    use grid::Row;
    use index::Column;

    #[test]
    fn line_length_works() {
        let template = Cell::default();
        let mut row = Row::new(Column(10), &template);
        row[Column(5)].c = 'a';

        assert_eq!(row.line_length(), Column(6));
    }

    #[test]
    fn line_length_works_with_wrapline() {
        let template = Cell::default();
        let mut row = Row::new(Column(10), &template);
        row[Column(9)].flags.insert(super::Flags::WRAPLINE);

        assert_eq!(row.line_length(), Column(10));
    }
}

#[cfg(all(test, feature = "bench"))]
mod benches {
    extern crate test;
    use super::Cell;

    #[bench]
    fn cell_reset(b: &mut test::Bencher) {
        b.iter(|| {
            let mut cell = Cell::default();

            for _ in 0..100 {
                cell.reset(test::black_box(&Cell::default()));
            }

            test::black_box(cell);
        });
    }
}
