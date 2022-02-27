use std::boxed::Box;
use std::fmt;

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
    pub struct ColorTy : u8 {
        const NAMED   = 0b0000_0000;
        const INDEXED = 0b0000_0001;
        const SPEC    = 0b0000_0010;
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
struct ColorMask {
    mask: u8,
}

impl ColorMask {
    /// The offset in bits for `bg` color type in mask.
    const BG_OFFSET: u8 = 2;
    /// Whether the underline was set.
    const HAS_UNDERLINE: u8 = 0b1000_0000;
    /// Reset type mask.
    const RESET_MASK: u8 = 0b0000_0011;
    /// The offset in bits for `fg` color type in mask.
    const UNDERLINE_OFFSET: u8 = 4;

    #[inline]
    fn new() -> Self {
        Default::default()
    }

    #[inline]
    fn fg_ty(&self) -> ColorTy {
        ColorTy::from_bits(self.mask & Self::RESET_MASK).unwrap()
    }

    #[inline]
    fn bg_ty(&self) -> ColorTy {
        ColorTy::from_bits(self.mask >> Self::BG_OFFSET & Self::RESET_MASK).unwrap()
    }

    #[inline]
    fn underline_ty(&self) -> ColorTy {
        ColorTy::from_bits(self.mask >> Self::UNDERLINE_OFFSET & Self::RESET_MASK).unwrap()
    }

    #[inline]
    fn set_fg_ty(&mut self, ty: ColorTy) {
        self.mask &= !(Self::RESET_MASK);
        self.mask |= ty.bits();
    }

    #[inline]
    fn set_bg_ty(&mut self, ty: ColorTy) {
        self.mask &= !(Self::RESET_MASK << Self::BG_OFFSET);
        self.mask |= ty.bits() << Self::BG_OFFSET;
    }

    #[inline]
    fn set_underline_ty(&mut self, ty: ColorTy) {
        self.mask |= Self::HAS_UNDERLINE;
        self.mask &= !(Self::RESET_MASK << Self::UNDERLINE_OFFSET);
        self.mask |= ty.bits() << Self::UNDERLINE_OFFSET;
    }

    #[inline]
    fn clear_underline(&mut self) {
        self.mask &= !Self::HAS_UNDERLINE;
    }

    #[inline]
    fn has_underline(&self) -> bool {
        self.mask & Self::HAS_UNDERLINE != 0
    }
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub union PackedColor {
    indexed: u8,
    named: NamedColor,
    rgb: Rgb,
}

impl PackedColor {
    #[inline]
    fn into_color(self, ty: ColorTy) -> Color {
        // SAFETY: The acess is type checked.
        unsafe {
            if ty.contains(ColorTy::SPEC) {
                Color::Spec(self.rgb)
            } else if ty.contains(ColorTy::INDEXED) {
                Color::Indexed(self.indexed)
            } else {
                Color::Named(self.named)
            }
        }
    }
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
        Self { mask: ColorMask::new(), fg, bg, underline }
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
        s.serialize_field("underline", &self.underline())?;
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
            underline: Option<Color>,
        }

        let unpacked = UnpackedColors::deserialize(deserializer)?;
        let mut cell_colors = CellColors::default();
        cell_colors.set_fg(unpacked.fg);
        cell_colors.set_bg(unpacked.bg);
        cell_colors.set_underline_color(unpacked.underline);
        Ok(cell_colors)
    }
}

impl Color {
    #[inline]
    fn into_packed_with_type(self) -> (ColorTy, PackedColor) {
        match self {
            Self::Spec(rgb) => (ColorTy::SPEC, PackedColor { rgb }),
            Self::Indexed(indexed) => (ColorTy::INDEXED, PackedColor { indexed }),
            Self::Named(named) => (ColorTy::NAMED, PackedColor { named }),
        }
    }
}

impl CellColors {
    #[inline]
    pub fn set_fg(&mut self, color: Color) {
        let (ty, color) = color.into_packed_with_type();
        self.fg = color;
        self.mask.set_fg_ty(ty);
    }

    #[inline]
    pub fn set_bg(&mut self, color: Color) {
        let (ty, color) = color.into_packed_with_type();
        self.bg = color;
        self.mask.set_bg_ty(ty);
    }

    #[inline]
    pub fn set_underline_color(&mut self, color: Option<Color>) {
        let (ty, color) = match color {
            Some(color) => color.into_packed_with_type(),
            None => {
                self.mask.clear_underline();
                return;
            },
        };
        self.underline = color;
        self.mask.set_underline_ty(ty);
    }

    #[inline]
    pub fn fg(&self) -> Color {
        self.fg.into_color(self.mask.fg_ty())
    }

    #[inline]
    pub fn bg(&self) -> Color {
        self.bg.into_color(self.mask.bg_ty())
    }

    #[inline]
    pub fn underline(&self) -> Option<Color> {
        if self.mask.has_underline() {
            Some(self.underline.into_color(self.mask.underline_ty()))
        } else {
            None
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
