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


use std::mem;

use ansi;
use Rgb;

bitflags! {
    #[derive(Serialize, Deserialize)]
    pub flags Flags: u32 {
        const INVERSE   = 0b00000001,
        const BOLD      = 0b00000010,
        const ITALIC    = 0b00000100,
        const UNDERLINE = 0b00001000,
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    Rgb(Rgb),
    Ansi(ansi::Color),
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
}

impl Cell {
    pub fn bold(&self) -> bool {
        self.flags.contains(BOLD)
    }

    pub fn new(c: char, fg: Color, bg: Color) -> Cell {
        Cell {
            c: c.into(),
            bg: bg,
            fg: fg,
            flags: Flags::empty(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.c == ' ' &&
            self.bg == Color::Ansi(ansi::Color::Background) &&
            !self.flags.contains(INVERSE)
    }

    #[inline]
    pub fn reset(&mut self, template: &Cell) {
        // memcpy template to self
        *self = template.clone();
    }

    #[inline]
    pub fn swap_fg_and_bg(&mut self) {
        mem::swap(&mut self.fg, &mut self.bg);
    }
}
