use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer};

use alacritty_config_derive::ConfigDeserialize;
use alacritty_terminal::term::color::{CellRgb, Rgb};

#[derive(ConfigDeserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Colors {
    pub primary: PrimaryColors,
    pub cursor: InvertedCellColors,
    pub vi_mode_cursor: InvertedCellColors,
    pub selection: InvertedCellColors,
    pub normal: NormalColors,
    pub bright: BrightColors,
    pub dim: Option<DimColors>,
    pub indexed_colors: Vec<IndexedColor>,
    pub search: SearchColors,
    pub line_indicator: LineIndicatorColors,
    pub hints: HintColors,
    pub transparent_background_colors: bool,
    footer_bar: BarColors,
}

impl Colors {
    pub fn footer_bar_foreground(&self) -> Rgb {
        self.search.bar.foreground.or(self.footer_bar.foreground).unwrap_or(self.primary.background)
    }

    pub fn footer_bar_background(&self) -> Rgb {
        self.search.bar.background.or(self.footer_bar.background).unwrap_or(self.primary.foreground)
    }
}

#[derive(ConfigDeserialize, Copy, Clone, Default, Debug, PartialEq, Eq)]
pub struct LineIndicatorColors {
    pub foreground: Option<Rgb>,
    pub background: Option<Rgb>,
}

#[derive(ConfigDeserialize, Default, Copy, Clone, Debug, PartialEq, Eq)]
pub struct HintColors {
    pub start: HintStartColors,
    pub end: HintEndColors,
}

#[derive(ConfigDeserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub struct HintStartColors {
    pub foreground: CellRgb,
    pub background: CellRgb,
}

impl Default for HintStartColors {
    fn default() -> Self {
        Self {
            foreground: CellRgb::Rgb(Rgb::new(0x1d, 0x1f, 0x21)),
            background: CellRgb::Rgb(Rgb::new(0xe9, 0xff, 0x5e)),
        }
    }
}

#[derive(ConfigDeserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub struct HintEndColors {
    pub foreground: CellRgb,
    pub background: CellRgb,
}

impl Default for HintEndColors {
    fn default() -> Self {
        Self {
            foreground: CellRgb::Rgb(Rgb::new(0xe9, 0xff, 0x5e)),
            background: CellRgb::Rgb(Rgb::new(0x1d, 0x1f, 0x21)),
        }
    }
}

#[derive(Deserialize, Copy, Clone, Default, Debug, PartialEq, Eq)]
pub struct IndexedColor {
    pub color: Rgb,

    index: ColorIndex,
}

impl IndexedColor {
    #[inline]
    pub fn index(&self) -> u8 {
        self.index.0
    }
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
struct ColorIndex(u8);

impl<'de> Deserialize<'de> for ColorIndex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let index = u8::deserialize(deserializer)?;

        if index < 16 {
            Err(SerdeError::custom(
                "Config error: indexed_color's index is {}, but a value bigger than 15 was \
                 expected; ignoring setting",
            ))
        } else {
            Ok(Self(index))
        }
    }
}

#[derive(ConfigDeserialize, Debug, Copy, Clone, PartialEq, Eq)]
pub struct InvertedCellColors {
    #[config(alias = "text")]
    pub foreground: CellRgb,
    #[config(alias = "cursor")]
    pub background: CellRgb,
}

impl Default for InvertedCellColors {
    fn default() -> Self {
        Self { foreground: CellRgb::CellBackground, background: CellRgb::CellForeground }
    }
}

#[derive(ConfigDeserialize, Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct SearchColors {
    pub focused_match: FocusedMatchColors,
    pub matches: MatchColors,
    #[config(deprecated = "use `colors.footer_bar` instead")]
    bar: BarColors,
}

#[derive(ConfigDeserialize, Debug, Copy, Clone, PartialEq, Eq)]
pub struct FocusedMatchColors {
    pub foreground: CellRgb,
    pub background: CellRgb,
}

impl Default for FocusedMatchColors {
    fn default() -> Self {
        Self {
            background: CellRgb::Rgb(Rgb::new(0x00, 0x00, 0x00)),
            foreground: CellRgb::Rgb(Rgb::new(0xff, 0xff, 0xff)),
        }
    }
}

#[derive(ConfigDeserialize, Debug, Copy, Clone, PartialEq, Eq)]
pub struct MatchColors {
    pub foreground: CellRgb,
    pub background: CellRgb,
}

impl Default for MatchColors {
    fn default() -> Self {
        Self {
            background: CellRgb::Rgb(Rgb::new(0xff, 0xff, 0xff)),
            foreground: CellRgb::Rgb(Rgb::new(0x00, 0x00, 0x00)),
        }
    }
}

#[derive(ConfigDeserialize, Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct BarColors {
    foreground: Option<Rgb>,
    background: Option<Rgb>,
}

#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct PrimaryColors {
    pub foreground: Rgb,
    pub background: Rgb,
    pub bright_foreground: Option<Rgb>,
    pub dim_foreground: Option<Rgb>,
}

impl Default for PrimaryColors {
    fn default() -> Self {
        PrimaryColors {
            background: Rgb::new(0x1d, 0x1f, 0x21),
            foreground: Rgb::new(0xc5, 0xc8, 0xc6),
            bright_foreground: Default::default(),
            dim_foreground: Default::default(),
        }
    }
}

#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct NormalColors {
    pub black: Rgb,
    pub red: Rgb,
    pub green: Rgb,
    pub yellow: Rgb,
    pub blue: Rgb,
    pub magenta: Rgb,
    pub cyan: Rgb,
    pub white: Rgb,
}

impl Default for NormalColors {
    fn default() -> Self {
        NormalColors {
            black: Rgb::new(0x1d, 0x1f, 0x21),
            red: Rgb::new(0xcc, 0x66, 0x66),
            green: Rgb::new(0xb5, 0xbd, 0x68),
            yellow: Rgb::new(0xf0, 0xc6, 0x74),
            blue: Rgb::new(0x81, 0xa2, 0xbe),
            magenta: Rgb::new(0xb2, 0x94, 0xbb),
            cyan: Rgb::new(0x8a, 0xbe, 0xb7),
            white: Rgb::new(0xc5, 0xc8, 0xc6),
        }
    }
}

#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct BrightColors {
    pub black: Rgb,
    pub red: Rgb,
    pub green: Rgb,
    pub yellow: Rgb,
    pub blue: Rgb,
    pub magenta: Rgb,
    pub cyan: Rgb,
    pub white: Rgb,
}

impl Default for BrightColors {
    fn default() -> Self {
        BrightColors {
            black: Rgb::new(0x66, 0x66, 0x66),
            red: Rgb::new(0xd5, 0x4e, 0x53),
            green: Rgb::new(0xb9, 0xca, 0x4a),
            yellow: Rgb::new(0xe7, 0xc5, 0x47),
            blue: Rgb::new(0x7a, 0xa6, 0xda),
            magenta: Rgb::new(0xc3, 0x97, 0xd8),
            cyan: Rgb::new(0x70, 0xc0, 0xb1),
            white: Rgb::new(0xea, 0xea, 0xea),
        }
    }
}

#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct DimColors {
    pub black: Rgb,
    pub red: Rgb,
    pub green: Rgb,
    pub yellow: Rgb,
    pub blue: Rgb,
    pub magenta: Rgb,
    pub cyan: Rgb,
    pub white: Rgb,
}

impl Default for DimColors {
    fn default() -> Self {
        DimColors {
            black: Rgb::new(0x13, 0x14, 0x15),
            red: Rgb::new(0x86, 0x43, 0x43),
            green: Rgb::new(0x77, 0x7c, 0x44),
            yellow: Rgb::new(0x9e, 0x82, 0x4c),
            blue: Rgb::new(0x55, 0x6a, 0x7d),
            magenta: Rgb::new(0x75, 0x61, 0x7b),
            cyan: Rgb::new(0x5b, 0x7d, 0x78),
            white: Rgb::new(0x82, 0x84, 0x82),
        }
    }
}
