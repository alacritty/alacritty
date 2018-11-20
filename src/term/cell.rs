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
use std::fmt;

use smallvec::SmallVec;
use serde::de::{SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Serialize, Deserialize, Serializer, Deserializer};

use ansi::{NamedColor, Color};
use grid;
use index::Column;

bitflags! {
    #[derive(Serialize, Deserialize)]
    pub struct Flags: u32 {
        const INVERSE           = 0b0_0000_0001;
        const BOLD              = 0b0_0000_0010;
        const ITALIC            = 0b0_0000_0100;
        const UNDERLINE         = 0b0_0000_1000;
        const WRAPLINE          = 0b0_0001_0000;
        const WIDE_CHAR         = 0b0_0010_0000;
        const WIDE_CHAR_SPACER  = 0b0_0100_0000;
        const DIM               = 0b0_1000_0000;
        const DIM_BOLD          = 0b0_1000_0010;
        const HIDDEN            = 0b1_0000_0000;
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CellContent {
    SingleChar(char),
    MultiChar([char; 3]),
}

impl CellContent {
    #[inline]
    fn is_empty(&self) -> bool {
        match *self {
            CellContent::SingleChar(c) => c == ' ',
            CellContent::MultiChar(_) => false,
        }
    }

    #[inline]
    pub fn primary(&self) -> char {
        match self {
            CellContent::SingleChar(c) => *c,
            CellContent::MultiChar(c) => c[0],
        }
    }

    #[inline]
    pub fn iter<'a>(&'a self) -> CellContentIter<'a> {
        CellContentIter::new(self)
    }
}

impl Serialize for CellContent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            CellContent::SingleChar(c) => serializer.serialize_char(*c),
            CellContent::MultiChar(c) => {
                let mut seq = serializer.serialize_seq(Some(c.len()))?;
                for element in c {
                    seq.serialize_element(&element)?;
                }
                seq.end()
            },
        }
    }
}

impl<'de> Deserialize<'de> for CellContent {
    fn deserialize<D>(deserializer: D) -> Result<CellContent, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct CellContentVisitor;

        impl<'a> Visitor<'a> for CellContentVisitor {
            type Value = CellContent;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "a char or an array of chars")
            }

            fn visit_char<E>(self, value: char) -> ::std::result::Result<CellContent, E>
                where E: ::serde::de::Error
            {
                Ok(CellContent::SingleChar(value))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'a>,
            {
                let mut array = [' ', 3];
                let index = 0;
                while let Some(value) = seq.next_element::<char>()? {
                    array[index] = value;
                }
                Ok(CellContent::MultiChar(array))
            }
        }

        deserializer.deserialize_any(CellContentVisitor)
    }
}

pub struct CellContentIter<'a> {
    inner: &'a CellContent,
    index: usize,
}

impl<'a> CellContentIter<'a> {
    fn new(inner: &'a CellContent) -> CellContentIter<'a> {
        CellContentIter {
            inner,
            index: 0,
        }
    }
}

impl<'a> Iterator for CellContentIter<'a> {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        let res = match self.inner {
            CellContent::SingleChar(c) => if self.index > 0 {
                None
            } else {
                Some(*c)
            },
            CellContent::MultiChar(c) => if self.index >= c.len() {
                None
            } else {
                Some(c[self.index])
            }
        };
        self.index += 1;
        res
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Cell {
    pub c: CellContent,
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
            if !cell.c.is_empty() {
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
            c: CellContent::SingleChar(c),
            bg,
            fg,
            flags: Flags::empty(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.c.is_empty()
            && self.bg == Color::Named(NamedColor::Background)
            && !self.flags.intersects(Flags::INVERSE | Flags::UNDERLINE)
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
