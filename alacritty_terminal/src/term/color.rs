use std::fmt::{self, Display, Formatter};
use std::ops::{Add, Index, Mul};
use std::str::FromStr;

use log::trace;
use serde::de::{Error as _, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_yaml::Value;

use crate::ansi::NamedColor;
use crate::config::Colors;

pub const COUNT: usize = 269;

/// Factor for automatic computation of dim colors used by terminal.
pub const DIM_FACTOR: f32 = 0.66;

#[derive(Debug, Eq, PartialEq, Copy, Clone, Default, Serialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// Implementation of W3C's luminance algorithm:
    /// https://www.w3.org/TR/WCAG20/#relativeluminancedef
    fn luminance(self) -> f64 {
        let channel_luminance = |channel| {
            let channel = channel as f64 / 255.;
            if channel <= 0.03928 {
                channel / 12.92
            } else {
                f64::powf((channel + 0.055) / 1.055, 2.4)
            }
        };

        let r_luminance = channel_luminance(self.r);
        let g_luminance = channel_luminance(self.g);
        let b_luminance = channel_luminance(self.b);

        0.2126 * r_luminance + 0.7152 * g_luminance + 0.0722 * b_luminance
    }

    /// Implementation of W3C's contrast algorithm:
    /// https://www.w3.org/TR/WCAG20/#contrast-ratiodef
    pub fn contrast(self, other: Rgb) -> f64 {
        let self_luminance = self.luminance();
        let other_luminance = other.luminance();

        let (darker, lighter) = if self_luminance > other_luminance {
            (other_luminance, self_luminance)
        } else {
            (self_luminance, other_luminance)
        };

        (lighter + 0.05) / (darker + 0.05)
    }
}

// A multiply function for Rgb, as the default dim is just *2/3.
impl Mul<f32> for Rgb {
    type Output = Rgb;

    fn mul(self, rhs: f32) -> Rgb {
        let result = Rgb {
            r: (f32::from(self.r) * rhs).max(0.0).min(255.0) as u8,
            g: (f32::from(self.g) * rhs).max(0.0).min(255.0) as u8,
            b: (f32::from(self.b) * rhs).max(0.0).min(255.0) as u8,
        };

        trace!("Scaling RGB by {} from {:?} to {:?}", rhs, self, result);

        result
    }
}

impl Add<Rgb> for Rgb {
    type Output = Rgb;

    fn add(self, rhs: Rgb) -> Rgb {
        Rgb {
            r: self.r.saturating_add(rhs.r),
            g: self.g.saturating_add(rhs.g),
            b: self.b.saturating_add(rhs.b),
        }
    }
}

/// Deserialize an Rgb from a hex string.
///
/// This is *not* the deserialize impl for Rgb since we want a symmetric
/// serialize/deserialize impl for ref tests.
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

        impl<'a> Visitor<'a> for RgbVisitor {
            type Value = Rgb;

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("hex color like #ff00ff")
            }

            fn visit_str<E>(self, value: &str) -> Result<Rgb, E>
            where
                E: serde::de::Error,
            {
                Rgb::from_str(&value[..]).map_err(|_| {
                    E::custom(format!(
                        "failed to parse rgb color {}; expected hex color like #ff00ff",
                        value
                    ))
                })
            }
        }

        // Return an error if the syntax is incorrect.
        let value = Value::deserialize(deserializer)?;

        // Attempt to deserialize from struct form.
        if let Ok(RgbDerivedDeser { r, g, b }) = RgbDerivedDeser::deserialize(value.clone()) {
            return Ok(Rgb { r, g, b });
        }

        // Deserialize from hex notation (either 0xff00ff or #ff00ff).
        value.deserialize_str(RgbVisitor).map_err(D::Error::custom)
    }
}

impl Display for Rgb {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
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
                Ok(Rgb { r, g, b })
            },
            Err(_) => Err(()),
        }
    }
}

/// RGB color optionally referencing the cell's foreground or background.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CellRgb {
    CellForeground,
    CellBackground,
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
        impl<'a> Visitor<'a> for CellRgbVisitor {
            type Value = CellRgb;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

                Rgb::from_str(&value[..]).map(CellRgb::Rgb).map_err(|_| {
                    E::custom(format!("failed to parse color {}; expected {}", value, EXPECTING))
                })
            }
        }

        deserializer.deserialize_str(CellRgbVisitor).map_err(D::Error::custom)
    }
}

/// List of indexed colors.
///
/// The first 16 entries are the standard ansi named colors. Items 16..232 are
/// the color cube.  Items 233..256 are the grayscale ramp. Item 256 is
/// the configured foreground color, item 257 is the configured background
/// color, item 258 is the cursor color. Following that are 8 positions for dim colors.
/// Item 267 is the bright foreground color, 268 the dim foreground.
#[derive(Copy, Clone)]
pub struct List {
    /// Indexed terminal colors.
    colors: [Rgb; COUNT],

    /// Color value changes from escape sequences.
    modified: [Option<Rgb>; COUNT],
}

impl<'a> From<&'a Colors> for List {
    fn from(colors: &Colors) -> List {
        let mut list = List { colors: [Rgb::default(); COUNT], modified: [None; COUNT] };

        list.fill_named(colors);
        list.fill_cube(colors);
        list.fill_gray_ramp(colors);

        list
    }
}

impl List {
    /// Get the current value of a color.
    pub fn get(&self, index: usize) -> &Rgb {
        self.modified[index].as_ref().unwrap_or(&self.colors[index])
    }

    pub fn get_modified(&self, index: usize) -> Option<Rgb> {
        self.modified[index]
    }

    /// Set the current value of a color.
    pub fn set(&mut self, index: usize, color: Rgb) {
        self.modified[index] = Some(color);
    }

    /// Reset the value of a color to the default.
    pub fn reset(&mut self, index: usize) {
        self.modified[index] = None;
    }

    /// Update the default colors.
    ///
    /// This modifies the default colors without changing the values that have been set using
    /// escape sequences.
    pub fn update_defaults(&mut self, colors: &Colors) {
        let list = Self::from(colors);
        self.colors = list.colors;
    }

    pub fn fill_named(&mut self, colors: &Colors) {
        // Normals.
        self.colors[NamedColor::Black as usize] = colors.normal.black;
        self.colors[NamedColor::Red as usize] = colors.normal.red;
        self.colors[NamedColor::Green as usize] = colors.normal.green;
        self.colors[NamedColor::Yellow as usize] = colors.normal.yellow;
        self.colors[NamedColor::Blue as usize] = colors.normal.blue;
        self.colors[NamedColor::Magenta as usize] = colors.normal.magenta;
        self.colors[NamedColor::Cyan as usize] = colors.normal.cyan;
        self.colors[NamedColor::White as usize] = colors.normal.white;

        // Brights.
        self.colors[NamedColor::BrightBlack as usize] = colors.bright.black;
        self.colors[NamedColor::BrightRed as usize] = colors.bright.red;
        self.colors[NamedColor::BrightGreen as usize] = colors.bright.green;
        self.colors[NamedColor::BrightYellow as usize] = colors.bright.yellow;
        self.colors[NamedColor::BrightBlue as usize] = colors.bright.blue;
        self.colors[NamedColor::BrightMagenta as usize] = colors.bright.magenta;
        self.colors[NamedColor::BrightCyan as usize] = colors.bright.cyan;
        self.colors[NamedColor::BrightWhite as usize] = colors.bright.white;
        self.colors[NamedColor::BrightForeground as usize] =
            colors.primary.bright_foreground.unwrap_or(colors.primary.foreground);

        // Foreground and background.
        self.colors[NamedColor::Foreground as usize] = colors.primary.foreground;
        self.colors[NamedColor::Background as usize] = colors.primary.background;

        // Dims.
        self.colors[NamedColor::DimForeground as usize] =
            colors.primary.dim_foreground.unwrap_or(colors.primary.foreground * DIM_FACTOR);
        match colors.dim {
            Some(ref dim) => {
                trace!("Using config-provided dim colors");
                self.colors[NamedColor::DimBlack as usize] = dim.black;
                self.colors[NamedColor::DimRed as usize] = dim.red;
                self.colors[NamedColor::DimGreen as usize] = dim.green;
                self.colors[NamedColor::DimYellow as usize] = dim.yellow;
                self.colors[NamedColor::DimBlue as usize] = dim.blue;
                self.colors[NamedColor::DimMagenta as usize] = dim.magenta;
                self.colors[NamedColor::DimCyan as usize] = dim.cyan;
                self.colors[NamedColor::DimWhite as usize] = dim.white;
            },
            None => {
                trace!("Deriving dim colors from normal colors");
                self.colors[NamedColor::DimBlack as usize] = colors.normal.black * DIM_FACTOR;
                self.colors[NamedColor::DimRed as usize] = colors.normal.red * DIM_FACTOR;
                self.colors[NamedColor::DimGreen as usize] = colors.normal.green * DIM_FACTOR;
                self.colors[NamedColor::DimYellow as usize] = colors.normal.yellow * DIM_FACTOR;
                self.colors[NamedColor::DimBlue as usize] = colors.normal.blue * DIM_FACTOR;
                self.colors[NamedColor::DimMagenta as usize] = colors.normal.magenta * DIM_FACTOR;
                self.colors[NamedColor::DimCyan as usize] = colors.normal.cyan * DIM_FACTOR;
                self.colors[NamedColor::DimWhite as usize] = colors.normal.white * DIM_FACTOR;
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
                        self.colors[index] = indexed_color.color;
                    } else {
                        self.colors[index] = Rgb {
                            r: if r == 0 { 0 } else { r * 40 + 55 },
                            b: if b == 0 { 0 } else { b * 40 + 55 },
                            g: if g == 0 { 0 } else { g * 40 + 55 },
                        };
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
                self.colors[index] = indexed_color.color;
                index += 1;
                continue;
            }

            let value = i * 10 + 8;
            self.colors[index] = Rgb { r: value, g: value, b: value };
            index += 1;
        }

        debug_assert!(index == 256);
    }
}

impl fmt::Debug for List {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("List[..]")
    }
}

impl Index<usize> for List {
    type Output = Rgb;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        self.get(index)
    }
}

impl Index<u8> for List {
    type Output = Rgb;

    #[inline]
    fn index(&self, index: u8) -> &Self::Output {
        self.get(index as usize)
    }
}

impl Index<NamedColor> for List {
    type Output = Rgb;

    #[inline]
    fn index(&self, index: NamedColor) -> &Self::Output {
        self.get(index as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::f64::EPSILON;

    #[test]
    fn contrast() {
        let rgb1 = Rgb { r: 0xff, g: 0xff, b: 0xff };
        let rgb2 = Rgb { r: 0x00, g: 0x00, b: 0x00 };
        assert!((rgb1.contrast(rgb2) - 21.).abs() < EPSILON);

        let rgb1 = Rgb { r: 0xff, g: 0xff, b: 0xff };
        assert!((rgb1.contrast(rgb1) - 1.).abs() < EPSILON);

        let rgb1 = Rgb { r: 0xff, g: 0x00, b: 0xff };
        let rgb2 = Rgb { r: 0x00, g: 0xff, b: 0x00 };
        assert!((rgb1.contrast(rgb2) - 2.285_543_608_124_253_3).abs() < EPSILON);

        let rgb1 = Rgb { r: 0x12, g: 0x34, b: 0x56 };
        let rgb2 = Rgb { r: 0xfe, g: 0xdc, b: 0xba };
        assert!((rgb1.contrast(rgb2) - 9.786_558_997_257_74).abs() < EPSILON);
    }
}
