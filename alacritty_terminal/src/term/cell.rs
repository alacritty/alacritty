use std::boxed::Box;
use std::{fmt, mem};

use bitflags::bitflags;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ansi::{Color, NamedColor};
use crate::grid::{self, GridCell};
use crate::index::Column;
use crate::term::color::Rgb;

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
        const UNDERCURL                 = 0b0001_0000_0000_0000;
        const DOTTED_UNDERLINE          = 0b0010_0000_0000_0000;
        const DASHED_UNDERLINE          = 0b0100_0000_0000_0000;
        const ALL_UNDERLINES            = Self::UNDERLINE.bits | Self::DOUBLE_UNDERLINE.bits
                                        | Self::UNDERCURL.bits | Self::DOTTED_UNDERLINE.bits
                                        | Self::DASHED_UNDERLINE.bits;
    }
}

/// Trait for determining if a reset should be performed.
pub trait ResetDiscriminant<T> {
    /// Value based on which equality for the reset will be determined.
    fn discriminant(&self) -> T;
}

impl<T: Copy> ResetDiscriminant<T> for T {
    fn discriminant(&self) -> T {
        *self
    }
}

impl ResetDiscriminant<Color> for Cell {
    fn discriminant(&self) -> Color {
        self.colors.bg()
    }
}

bitflags! {
    pub struct ColorMask : u8 {
        const DEFUALT_COLORS        = 0b0000_00_00;

        // Foreground color.
        const RESET_FOREGROUND      = 0b1111_11_00;
        const FOREGROUND_NAMED      = 0b0000_00_00;
        const FOREGROUND_INDEXED    = 0b0000_00_01;
        const FOREGROUND_SPEC       = 0b0000_00_10;

        // Background color.
        const RESET_BACKGROUND      = 0b1111_00_11;
        const BACKGROUND_NAMED      = 0b0000_00_00;
        const BACKGROUND_INDEXED    = 0b0000_01_00;
        const BACKGROUND_SPEC       = 0b0000_10_00;

        // NOTE we use three bits for underline color to express absence of it.
        const RESET_UNDERLINE      = 0b1000_11_11;
        const UNDERLINE_NAMED      = 0b0001_00_00;
        const UNDERLINE_INDEXED    = 0b0010_00_00;
        const UNDERLINE_SPEC       = 0b0100_00_00;
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub union PackedColor {
    indexed: u8,
    named: NamedColor,
    rgb: Rgb,
}

/// Packed storage for the colors cell is using.
#[repr(packed)]
#[derive(Copy, Clone)]
pub struct CellColors {
    /// A common mask for all colors in the cell.
    mask: ColorMask,

    /// Cell foreground color.
    fg: PackedColor,

    /// Cell background color.
    bg: PackedColor,

    /// Underline color.
    underline: PackedColor,
}

impl fmt::Debug for CellColors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CellColors")
            .field("mask", &self.mask)
            .field("fg", &self.fg())
            .field("bg", &self.bg())
            .finish()
    }
}

impl Default for CellColors {
    #[inline]
    fn default() -> Self {
        let fg = PackedColor { named: NamedColor::Foreground };
        let bg = PackedColor { named: NamedColor::Background };
        let underline = PackedColor { rgb: Rgb::default() };
        Self { mask: ColorMask::DEFUALT_COLORS, fg, bg, underline }
    }
}

impl PartialEq for CellColors {
    fn eq(&self, other: &Self) -> bool {
        self.mask == other.mask
            && self.fg() == other.fg()
            && self.bg() == other.bg()
            && self.underline() == other.underline()
    }
}
impl Eq for CellColors {}

impl Serialize for CellColors {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("CellColors", 2)?;
        s.serialize_field("fg", &self.fg())?;
        s.serialize_field("bg", &self.bg())?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for CellColors {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct UnpackedColors {
            fg: Color,
            bg: Color,
        }

        let unpacked = UnpackedColors::deserialize(deserializer)?;
        let mut cell_colors = CellColors::default();
        cell_colors.set_fg(unpacked.fg);
        cell_colors.set_bg(unpacked.bg);
        Ok(cell_colors)
    }
}

impl CellColors {
    #[inline]
    pub fn set_fg(&mut self, color: Color) {
        self.mask &= ColorMask::RESET_FOREGROUND;
        self.fg = unsafe { mem::zeroed() };
        match color {
            Color::Spec(rgb) => {
                self.mask |= ColorMask::FOREGROUND_SPEC;
                self.fg.rgb = rgb;
            },
            Color::Indexed(index) => {
                self.mask |= ColorMask::FOREGROUND_INDEXED;
                self.fg.indexed = index;
            },
            Color::Named(named) => {
                self.mask |= ColorMask::FOREGROUND_NAMED;
                self.fg.named = named;
            },
        }
    }

    #[inline]
    pub fn set_bg(&mut self, color: Color) {
        self.mask &= ColorMask::RESET_BACKGROUND;
        self.bg = unsafe { mem::zeroed() };
        match color {
            Color::Spec(rgb) => {
                self.mask |= ColorMask::BACKGROUND_SPEC;
                self.bg.rgb = rgb;
            },
            Color::Indexed(index) => {
                self.mask |= ColorMask::BACKGROUND_INDEXED;
                self.bg.indexed = index;
            },
            Color::Named(named) => {
                self.mask |= ColorMask::BACKGROUND_NAMED;
                self.bg.named = named;
            },
        }
    }

    #[inline]
    pub fn set_underline_color(&mut self, color: Option<Color>) {
        self.mask &= ColorMask::RESET_UNDERLINE;
        self.underline = unsafe { mem::zeroed() };
        let color = match color {
            Some(color) => color,
            None => return,
        };

        match color {
            Color::Spec(rgb) => {
                self.mask |= ColorMask::UNDERLINE_SPEC;
                self.underline.rgb = rgb;
            },
            Color::Named(named) => {
                self.mask |= ColorMask::UNDERLINE_NAMED;
                self.underline.named = named;
            },
            Color::Indexed(index) => {
                self.mask |= ColorMask::UNDERLINE_INDEXED;
                self.underline.indexed = index;
            },
        }
    }

    #[inline]
    pub fn fg(&self) -> Color {
        // SAFETY the mask is carrying the type of color.
        unsafe {
            if self.mask.intersects(ColorMask::FOREGROUND_SPEC) {
                Color::Spec(self.fg.rgb)
            } else if self.mask.intersects(ColorMask::FOREGROUND_INDEXED) {
                Color::Indexed(self.fg.indexed)
            } else {
                Color::Named(self.fg.named)
            }
        }
    }

    #[inline]
    pub fn bg(&self) -> Color {
        // SAFETY the mask is carrying the type of color.
        unsafe {
            if self.mask.contains(ColorMask::BACKGROUND_SPEC) {
                Color::Spec(self.bg.rgb)
            } else if self.mask.contains(ColorMask::BACKGROUND_INDEXED) {
                Color::Indexed(self.bg.indexed)
            } else {
                Color::Named(self.bg.named)
            }
        }
    }

    #[inline]
    pub fn underline(&self) -> Option<Color> {
        // If the mask for underline colors is empty there's no special color for underline.
        if self.mask & !ColorMask::RESET_UNDERLINE == ColorMask::DEFUALT_COLORS {
            return None;
        }

        // SAFETY the mask is carrying the type of color.
        unsafe {
            if self.mask.contains(ColorMask::UNDERLINE_SPEC) {
                Some(Color::Spec(self.underline.rgb))
            } else if self.mask.contains(ColorMask::UNDERLINE_INDEXED) {
                Some(Color::Indexed(self.underline.indexed))
            } else {
                Some(Color::Named(self.underline.named))
            }
        }
    }
}

/// Dynamically allocated cell content.
///
/// This storage is reserved for cell attributes which are rarely set. This allows reducing the
/// allocation required ahead of time for every cell, with some additional overhead when the extra
/// storage is actually required.
#[derive(Serialize, Deserialize, Default, Debug, Clone, Eq, PartialEq)]
struct CellExtra {
    zerowidth: Vec<char>,
}

/// Content and attributes of a single cell in the terminal grid.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Cell {
    pub c: char,
    #[serde(flatten)]
    pub colors: CellColors,
    pub flags: Flags,
    #[serde(default)]
    extra: Option<Box<CellExtra>>,
}

impl Default for Cell {
    #[inline]
    fn default() -> Cell {
        Cell { c: ' ', colors: Default::default(), flags: Flags::empty(), extra: None }
    }
}

impl Cell {
    /// Zerowidth characters stored in this cell.
    #[inline]
    pub fn zerowidth(&self) -> Option<&[char]> {
        self.extra.as_ref().map(|extra| extra.zerowidth.as_slice())
    }

    /// Write a new zerowidth character to this cell.
    #[inline]
    pub fn push_zerowidth(&mut self, c: char) {
        self.extra.get_or_insert_with(Default::default).zerowidth.push(c);
    }

    /// Free all dynamically allocated cell storage.
    #[inline]
    pub fn drop_extra(&mut self) {
        if self.extra.is_some() {
            self.extra = None;
        }
    }

    /// Remove all wide char data from a cell.
    #[inline(never)]
    pub fn clear_wide(&mut self) {
        self.flags.remove(Flags::WIDE_CHAR);
        self.drop_extra();
        self.c = ' ';
    }
}

impl GridCell for Cell {
    #[inline]
    fn is_empty(&self) -> bool {
        (self.c == ' ' || self.c == '\t')
            && self.colors == Default::default()
            && !self.flags.intersects(
                Flags::INVERSE
                    | Flags::ALL_UNDERLINES
                    | Flags::STRIKEOUT
                    | Flags::WRAPLINE
                    | Flags::WIDE_CHAR_SPACER
                    | Flags::LEADING_WIDE_CHAR_SPACER,
            )
            && self.extra.as_ref().map(|extra| extra.zerowidth.is_empty()) != Some(false)
    }

    #[inline]
    fn flags(&self) -> &Flags {
        &self.flags
    }

    #[inline]
    fn flags_mut(&mut self) -> &mut Flags {
        &mut self.flags
    }

    #[inline]
    fn reset(&mut self, template: &Self) {
        *self = template.colors.bg().into();
    }
}

impl From<Color> for Cell {
    #[inline]
    fn from(color: Color) -> Self {
        let mut cell = Cell::default();
        cell.colors.set_bg(color);
        cell
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
            if cell.c != ' '
                || cell.extra.as_ref().map(|extra| extra.zerowidth.is_empty()) == Some(false)
            {
                length = Column(self.len() - index);
                break;
            }
        }

        length
    }
}

#[cfg(test)]
mod tests {
    use super::{Cell, LineLength};

    use crate::grid::Row;
    use crate::index::Column;

    #[test]
    fn line_length_works() {
        let mut row = Row::<Cell>::new(10);
        row[Column(5)].c = 'a';

        assert_eq!(row.line_length(), Column(6));
    }

    #[test]
    fn line_length_works_with_wrapline() {
        let mut row = Row::<Cell>::new(10);
        row[Column(9)].flags.insert(super::Flags::WRAPLINE);

        assert_eq!(row.line_length(), Column(10));
    }
}
