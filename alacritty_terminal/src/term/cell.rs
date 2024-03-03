use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use bitflags::bitflags;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::grid::{self, GridCell};
use crate::index::Column;
use crate::vte::ansi::{Color, Hyperlink as VteHyperlink, NamedColor};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
        const ALL_UNDERLINES            = Self::UNDERLINE.bits() | Self::DOUBLE_UNDERLINE.bits()
                                        | Self::UNDERCURL.bits() | Self::DOTTED_UNDERLINE.bits()
                                        | Self::DASHED_UNDERLINE.bits();
    }
}

#[cfg(feature = "bidi_draft")]
bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    pub(super) struct BidiFlags: u8 {
        /// Set with:
        ///  `CSI 8 l`
        ///
        /// Reset with:
        ///  `CSI 8 h`
        ///
        /// Yes, the default is the high state (implicit mode).
        const EXPLICIT_DIRECTION = 0b0000_0001;
        /// Set with:
        ///  `CSI 2 SPACE k` (RTL).
        ///  `CSI 1 SPACE k` (LTR)
        ///
        /// Reset with:
        ///  `CSI 0 SPACE k` (default)
        ///  `CSI SPACE k`   (default)
        ///
        /// Default paragraph direction is not to be confused with
        /// auto-detection. The default is implementation defined, and
        /// is often LTR.
        const NON_DEFAULT_PARA_DIR = 0b0000_0010;
        /// Set with:
        ///  `CSI 2 SPACE k` (RTL)
        ///
        /// Reset with  with:
        ///  `CSI 1 SPACE k` (LTR)
        ///
        ///  Only in effect when `NON_DEFAULT_PARA_DIR` is set.
        ///  Only acts as fallback when `AUTO_PARA_DIR` is set.
        const RTL_PARA_DIR = 0b0000_0100;
        /// Set with:
        ///  `CSI ? 2501 h` (auto)
        ///
        /// Reset with:
        ///  `CSI ? 2501 l` (default or RTL/LTR)
        ///
        /// Set auto direction for paragraphs, based on their content's detected direction.
        /// Implicit paragraph direction is used if no specific direction is detected.
        /// This is ignored if `EXPLICIT_DIRECTION` is set.
        const AUTO_PARA_DIR = 0b0000_1000;

        /// Set with:
        ///  `CSI ? 2500 h` (mirroring)
        ///
        /// Reset with:
        ///  `CSI ? 2500 l` (no mirroring)
        ///
        /// Use mirrored glyphs of characters from the box drawing block
        /// (U+2500 - U+257F) in RTL spans.
        ///
        /// Visually mirror-able characters from that range don't have the `Bidi_Mirrored` property
        /// set. So a Bidi-aware shaper/renderer wouldn't mirror them on its own when detected in
        /// RTL spans.
        ///
        /// More info:
        ///  https://www.unicode.org/reports/tr9/#Mirroring
        ///  https://www.unicode.org/reports/tr9/#HL6
        ///
        /// Note that this will have an effect in RTL spans, irregardless of paragraph direction.
        const BOX_MIRRORING = 0b0001_0000;
    }
}

#[cfg(feature = "bidi_draft")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BidiDir {
    LTR,
    RTL,
    // Terminal-defined
    Default,
}

#[cfg(feature = "bidi_draft")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BidiMode {
    Explicit { forced_dir: BidiDir },
    Implicit { para_dir: BidiDir },
    Auto { fallback_para_dir: BidiDir },
}

#[cfg(feature = "bidi_draft")]
impl Default for BidiMode {
    fn default() -> Self {
        Self::Implicit { para_dir: BidiDir::Default }
    }
}

/// Counter for hyperlinks without explicit ID.
static HYPERLINK_ID_SUFFIX: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Hyperlink {
    inner: Arc<HyperlinkInner>,
}

impl Hyperlink {
    pub fn new<T: ToString>(id: Option<T>, uri: String) -> Self {
        let inner = Arc::new(HyperlinkInner::new(id, uri));
        Self { inner }
    }

    pub fn id(&self) -> &str {
        &self.inner.id
    }

    pub fn uri(&self) -> &str {
        &self.inner.uri
    }
}

impl From<VteHyperlink> for Hyperlink {
    fn from(value: VteHyperlink) -> Self {
        Self::new(value.id, value.uri)
    }
}

impl From<Hyperlink> for VteHyperlink {
    fn from(val: Hyperlink) -> Self {
        VteHyperlink { id: Some(val.id().to_owned()), uri: val.uri().to_owned() }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
struct HyperlinkInner {
    /// Identifier for the given hyperlink.
    id: String,

    /// Resource identifier of the hyperlink.
    uri: String,
}

impl HyperlinkInner {
    pub fn new<T: ToString>(id: Option<T>, uri: String) -> Self {
        let id = match id {
            Some(id) => id.to_string(),
            None => {
                let mut id = HYPERLINK_ID_SUFFIX.fetch_add(1, Ordering::Relaxed).to_string();
                id.push_str("_alacritty");
                id
            },
        };

        Self { id, uri }
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
        self.bg
    }
}

/// Dynamically allocated cell content.
///
/// This storage is reserved for cell attributes which are rarely set. This allows reducing the
/// allocation required ahead of time for every cell, with some additional overhead when the extra
/// storage is actually required.
#[derive(Default, Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CellExtra {
    zerowidth: Vec<char>,

    underline_color: Option<Color>,

    hyperlink: Option<Hyperlink>,

    #[cfg(feature = "bidi_draft")]
    pub(super) bidi_flags: BidiFlags,
}

/// Content and attributes of a single cell in the terminal grid.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
    pub extra: Option<Arc<CellExtra>>,
}

impl Default for Cell {
    #[inline]
    fn default() -> Cell {
        Cell {
            c: ' ',
            bg: Color::Named(NamedColor::Background),
            fg: Color::Named(NamedColor::Foreground),
            flags: Flags::empty(),
            extra: None,
        }
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
    pub fn push_zerowidth(&mut self, character: char) {
        let extra = self.extra.get_or_insert(Default::default());
        Arc::make_mut(extra).zerowidth.push(character);
    }

    /// Remove all wide char data from a cell.
    #[inline(never)]
    pub fn clear_wide(&mut self) {
        self.flags.remove(Flags::WIDE_CHAR);
        if let Some(extra) = self.extra.as_mut() {
            Arc::make_mut(extra).zerowidth = Vec::new();
        }
        self.c = ' ';
    }

    /// Set underline color on the cell.
    pub fn set_underline_color(&mut self, color: Option<Color>) {
        // If we reset color and we don't have zerowidth we should drop extra storage.
        if color.is_none()
            && self
                .extra
                .as_ref()
                .map_or(true, |extra| extra.zerowidth.is_empty() && extra.hyperlink.is_none())
        {
            self.extra = None;
        } else {
            let extra = self.extra.get_or_insert(Default::default());
            Arc::make_mut(extra).underline_color = color;
        }
    }

    /// Underline color stored in this cell.
    #[inline]
    pub fn underline_color(&self) -> Option<Color> {
        self.extra.as_ref()?.underline_color
    }

    /// Set hyperlink.
    pub fn set_hyperlink(&mut self, hyperlink: Option<Hyperlink>) {
        let should_drop = hyperlink.is_none()
            && self.extra.as_ref().map_or(true, |extra| {
                extra.zerowidth.is_empty() && extra.underline_color.is_none()
            });

        if should_drop {
            self.extra = None;
        } else {
            let extra = self.extra.get_or_insert(Default::default());
            Arc::make_mut(extra).hyperlink = hyperlink;
        }
    }

    /// Hyperlink stored in this cell.
    #[inline]
    pub fn hyperlink(&self) -> Option<Hyperlink> {
        self.extra.as_ref()?.hyperlink.clone()
    }
}

#[cfg(feature = "bidi_draft")]
impl Cell {
    pub(super) fn remove_bidi_flag(&mut self, bidi_flag: BidiFlags) {
        self.extra.as_mut().map(|extra| Arc::make_mut(extra).bidi_flags.remove(bidi_flag));
        let is_default_inner =
            self.extra.as_deref().map(|extra| *extra == Default::default()).unwrap_or(false);
        if is_default_inner {
            self.extra = None;
        }
    }

    pub(super) fn insert_bidi_flag(&mut self, bidi_flag: BidiFlags) {
        if bidi_flag.is_empty() && self.extra.is_none() {
            return;
        }

        let extra = self.extra.get_or_insert(Default::default());
        Arc::make_mut(extra).bidi_flags.insert(bidi_flag);
    }

    #[inline]
    pub(super) fn set_bidi_flags(&mut self, bidi_flags: BidiFlags) {
        if bidi_flags.is_empty() && self.extra.is_none() {
            return;
        }

        let extra = self.extra.get_or_insert(Default::default());
        Arc::make_mut(extra).bidi_flags = bidi_flags;
    }

    #[inline]
    pub(super) fn bidi_flags(&self) -> BidiFlags {
        self.extra.as_ref().map(|extra| extra.bidi_flags).unwrap_or_default()
    }

    #[inline]
    pub fn bidi_mode(&self) -> BidiMode {
        let bidi_flags = self.bidi_flags();
        let dir = if !bidi_flags.contains(BidiFlags::NON_DEFAULT_PARA_DIR) {
            BidiDir::Default
        } else if bidi_flags.contains(BidiFlags::RTL_PARA_DIR) {
            BidiDir::RTL
        } else {
            BidiDir::LTR
        };

        if bidi_flags.contains(BidiFlags::EXPLICIT_DIRECTION) {
            BidiMode::Explicit { forced_dir: dir }
        } else if bidi_flags.contains(BidiFlags::AUTO_PARA_DIR) {
            BidiMode::Auto { fallback_para_dir: dir }
        } else {
            BidiMode::Implicit { para_dir: dir }
        }
    }

    #[inline]
    pub fn bidi_box_mirroring(&self) -> bool {
        self.bidi_flags().contains(BidiFlags::BOX_MIRRORING)
    }
}

impl GridCell for Cell {
    #[inline]
    fn is_empty(&self) -> bool {
        (self.c == ' ' || self.c == '\t')
            && self.bg == Color::Named(NamedColor::Background)
            && self.fg == Color::Named(NamedColor::Foreground)
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
        *self = Cell { bg: template.bg, ..Cell::default() };
    }
}

impl From<Color> for Cell {
    #[inline]
    fn from(color: Color) -> Self {
        Self { bg: color, ..Cell::default() }
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
    use super::*;

    use std::mem;

    use crate::grid::Row;
    use crate::index::Column;

    #[test]
    fn cell_size_is_below_cap() {
        // Expected cell size on 64-bit architectures.
        const EXPECTED_CELL_SIZE: usize = 24;

        // Ensure that cell size isn't growing by accident.
        assert!(mem::size_of::<Cell>() <= EXPECTED_CELL_SIZE);
    }

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
