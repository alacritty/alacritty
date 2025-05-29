use std::fmt::{self, Display, Formatter};
use std::ops::{Add, Deref, Index, IndexMut, Mul};
use std::str::FromStr;

use log::trace;
use serde::de::{Error as SerdeError, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use alacritty_config_derive::SerdeReplace;
use alacritty_terminal::term::color::COUNT;
use alacritty_terminal::vte::ansi::{NamedColor, Rgb as VteRgb};

use crate::config::color::Colors;

/// Factor for automatic computation of dim colors.
pub const DIM_FACTOR: f32 = 0.66;

#[derive(Copy, Clone)]
pub struct List([Rgb; COUNT]);

impl From<&'_ Colors> for List {
    fn from(colors: &Colors) -> List {
        // Type inference fails without this annotation.
        let mut list = List([Rgb::default(); COUNT]);

        list.fill_named(colors);
        list.fill_cube(colors);
        list.fill_gray_ramp(colors);

        list
    }
}

impl List {
    pub fn fill_named(&mut self, colors: &Colors) {
        // Normals.
        self[NamedColor::Black] = colors.normal.black;
        self[NamedColor::Red] = colors.normal.red;
        self[NamedColor::Green] = colors.normal.green;
        self[NamedColor::Yellow] = colors.normal.yellow;
        self[NamedColor::Blue] = colors.normal.blue;
        self[NamedColor::Magenta] = colors.normal.magenta;
        self[NamedColor::Cyan] = colors.normal.cyan;
        self[NamedColor::White] = colors.normal.white;

        // Brights.
        self[NamedColor::BrightBlack] = colors.bright.black;
        self[NamedColor::BrightRed] = colors.bright.red;
        self[NamedColor::BrightGreen] = colors.bright.green;
        self[NamedColor::BrightYellow] = colors.bright.yellow;
        self[NamedColor::BrightBlue] = colors.bright.blue;
        self[NamedColor::BrightMagenta] = colors.bright.magenta;
        self[NamedColor::BrightCyan] = colors.bright.cyan;
        self[NamedColor::BrightWhite] = colors.bright.white;
        self[NamedColor::BrightForeground] =
            colors.primary.bright_foreground.unwrap_or(colors.primary.foreground);

        // Foreground and background.
        self[NamedColor::Foreground] = colors.primary.foreground;
        self[NamedColor::Background] = colors.primary.background;

        // Dims.
        self[NamedColor::DimForeground] =
            colors.primary.dim_foreground.unwrap_or(colors.primary.foreground * DIM_FACTOR);
        match colors.dim {
            Some(ref dim) => {
                trace!("Using config-provided dim colors");
                self[NamedColor::DimBlack] = dim.black;
                self[NamedColor::DimRed] = dim.red;
                self[NamedColor::DimGreen] = dim.green;
                self[NamedColor::DimYellow] = dim.yellow;
                self[NamedColor::DimBlue] = dim.blue;
                self[NamedColor::DimMagenta] = dim.magenta;
                self[NamedColor::DimCyan] = dim.cyan;
                self[NamedColor::DimWhite] = dim.white;
            },
            None => {
                trace!("Deriving dim colors from normal colors");
                self[NamedColor::DimBlack] = colors.normal.black * DIM_FACTOR;
                self[NamedColor::DimRed] = colors.normal.red * DIM_FACTOR;
                self[NamedColor::DimGreen] = colors.normal.green * DIM_FACTOR;
                self[NamedColor::DimYellow] = colors.normal.yellow * DIM_FACTOR;
                self[NamedColor::DimBlue] = colors.normal.blue * DIM_FACTOR;
                self[NamedColor::DimMagenta] = colors.normal.magenta * DIM_FACTOR;
                self[NamedColor::DimCyan] = colors.normal.cyan * DIM_FACTOR;
                self[NamedColor::DimWhite] = colors.normal.white * DIM_FACTOR;
            },
        }
    }

    pub fn fill_cube(&mut self, colors: &Colors) {
        let mut index: usize = 16;
        // Build colors.
        for r in 0..6 {
            for g in 0..6 {
                for b in 0..6 {
                    // Override colors 16..232 with the config (if present).
                    if let Some(indexed_color) =
                        colors.indexed_colors.iter().find(|ic| ic.index() == index as u8)
                    {
                        self[index] = indexed_color.color;
                    } else {
                        self[index] = Rgb::new(
                            if r == 0 { 0 } else { r * 40 + 55 },
                            if g == 0 { 0 } else { g * 40 + 55 },
                            if b == 0 { 0 } else { b * 40 + 55 },
                        );
                    }
                    index += 1;
                }
            }
        }

        debug_assert!(index == 232);
    }

    pub fn fill_gray_ramp(&mut self, colors: &Colors) {
        let mut index: usize = 232;

        for i in 0..24 {
            // Index of the color is number of named colors + number of cube colors + i.
            let color_index = 16 + 216 + i;

            // Override colors 232..256 with the config (if present).
            if let Some(indexed_color) =
                colors.indexed_colors.iter().find(|ic| ic.index() == color_index)
            {
                self[index] = indexed_color.color;
                index += 1;
                continue;
            }

            let value = i * 10 + 8;
            self[index] = Rgb::new(value, value, value);
            index += 1;
        }

        debug_assert!(index == 256);
    }
}

impl Index<usize> for List {
    type Output = Rgb;

    #[inline]
    fn index(&self, idx: usize) -> &Self::Output {
        &self.0[idx]
    }
}

impl IndexMut<usize> for List {
    #[inline]
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        &mut self.0[idx]
    }
}

impl Index<NamedColor> for List {
    type Output = Rgb;

    #[inline]
    fn index(&self, idx: NamedColor) -> &Self::Output {
        &self.0[idx as usize]
    }
}

impl IndexMut<NamedColor> for List {
    #[inline]
    fn index_mut(&mut self, idx: NamedColor) -> &mut Self::Output {
        &mut self.0[idx as usize]
    }
}

#[derive(SerdeReplace, Debug, Eq, PartialEq, Copy, Clone, Default)]
pub struct Rgb(pub VteRgb);

impl Rgb {
    #[inline]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self(VteRgb { r, g, b })
    }

    #[inline]
    pub fn as_tuple(self) -> (u8, u8, u8) {
        (self.0.r, self.0.g, self.0.b)
    }
}

impl From<VteRgb> for Rgb {
    fn from(value: VteRgb) -> Self {
        Self(value)
    }
}

impl Deref for Rgb {
    type Target = VteRgb;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Mul<f32> for Rgb {
    type Output = Rgb;

    fn mul(self, rhs: f32) -> Self::Output {
        Rgb(self.0 * rhs)
    }
}

impl Add<Rgb> for Rgb {
    type Output = Rgb;

    fn add(self, rhs: Rgb) -> Self::Output {
        Rgb(self.0 + rhs.0)
    }
}

/// Deserialize Rgb color from a hex string.
impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RgbVisitor;

        // Used for deserializing reftests.
        #[derive(Deserialize)]
        struct RgbDerivedDeser {
            r: u8,
            g: u8,
            b: u8,
        }

        impl Visitor<'_> for RgbVisitor {
            type Value = Rgb;

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("hex color like #ff00ff")
            }

            fn visit_str<E>(self, value: &str) -> Result<Rgb, E>
            where
                E: serde::de::Error,
            {
                Rgb::from_str(value).map_err(|_| {
                    E::custom(format!(
                        "failed to parse rgb color {value}; expected hex color like #ff00ff"
                    ))
                })
            }
        }

        // Return an error if the syntax is incorrect.
        let value = toml::Value::deserialize(deserializer)?;

        // Attempt to deserialize from struct form.
        if let Ok(RgbDerivedDeser { r, g, b }) = RgbDerivedDeser::deserialize(value.clone()) {
            return Ok(Rgb::new(r, g, b));
        }

        // Deserialize from hex notation (either 0xff00ff or #ff00ff).
        value.deserialize_str(RgbVisitor).map_err(D::Error::custom)
    }
}

/// Serialize Rgb color to a hex string.
impl Serialize for Rgb {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl FromStr for Rgb {
    type Err = ();

    fn from_str(s: &str) -> Result<Rgb, ()> {
        let chars = if s.starts_with("0x") && s.len() == 8 {
            &s[2..]
        } else if s.starts_with('#') && s.len() == 7 {
            &s[1..]
        } else {
            return Err(());
        };

        match u32::from_str_radix(chars, 16) {
            Ok(mut color) => {
                let b = (color & 0xff) as u8;
                color >>= 8;
                let g = (color & 0xff) as u8;
                color >>= 8;
                let r = color as u8;
                Ok(Rgb::new(r, g, b))
            },
            Err(_) => Err(()),
        }
    }
}

/// RGB color optionally referencing the cell's foreground or background.
#[derive(SerdeReplace, Serialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum CellRgb {
    CellForeground,
    CellBackground,
    #[serde(untagged)]
    Rgb(Rgb),
}

impl CellRgb {
    pub fn color(self, foreground: Rgb, background: Rgb) -> Rgb {
        match self {
            Self::CellForeground => foreground,
            Self::CellBackground => background,
            Self::Rgb(rgb) => rgb,
        }
    }
}

impl Default for CellRgb {
    fn default() -> Self {
        Self::Rgb(Rgb::default())
    }
}

impl<'de> Deserialize<'de> for CellRgb {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        const EXPECTING: &str = "CellForeground, CellBackground, or hex color like #ff00ff";

        struct CellRgbVisitor;
        impl Visitor<'_> for CellRgbVisitor {
            type Value = CellRgb;

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str(EXPECTING)
            }

            fn visit_str<E>(self, value: &str) -> Result<CellRgb, E>
            where
                E: serde::de::Error,
            {
                // Attempt to deserialize as enum constants.
                match value {
                    "CellForeground" => return Ok(CellRgb::CellForeground),
                    "CellBackground" => return Ok(CellRgb::CellBackground),
                    _ => (),
                }

                Rgb::from_str(value).map(CellRgb::Rgb).map_err(|_| {
                    E::custom(format!("failed to parse color {value}; expected {EXPECTING}"))
                })
            }
        }

        deserializer.deserialize_str(CellRgbVisitor).map_err(D::Error::custom)
    }
}
