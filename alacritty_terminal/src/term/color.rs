use std::fmt::{self, Display, Formatter};
use std::ops::{Add, Index, IndexMut, Mul};
use std::str::FromStr;

use log::trace;
use serde::de::{Error as _, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_yaml::Value;

use crate::ansi::NamedColor;

/// Number of terminal colors.
pub const COUNT: usize = 269;

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

    /// Implementation of [W3C's contrast algorithm].
    ///
    /// [W3C's contrast algorithm]: https://www.w3.org/TR/WCAG20/#contrast-ratiodef
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
                Rgb::from_str(value).map_err(|_| {
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

                Rgb::from_str(value).map(CellRgb::Rgb).map_err(|_| {
                    E::custom(format!("failed to parse color {}; expected {}", value, EXPECTING))
                })
            }
        }

        deserializer.deserialize_str(CellRgbVisitor).map_err(D::Error::custom)
    }
}

/// Array of indexed colors.
///
/// | Indices  | Description       |
/// | -------- | ----------------- |
/// | 0..16    | Named ANSI colors |
/// | 16..232  | Color cube        |
/// | 233..256 | Grayscale ramp    |
/// | 256      | Foreground        |
/// | 257      | Background        |
/// | 258      | Cursor            |
/// | 259..267 | Dim colors        |
/// | 267      | Bright foreground |
/// | 268      | Dim background    |
#[derive(Copy, Clone)]
pub struct Colors([Option<Rgb>; COUNT]);

impl Default for Colors {
    fn default() -> Self {
        Self([None; COUNT])
    }
}

impl Index<usize> for Colors {
    type Output = Option<Rgb>;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<usize> for Colors {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl Index<NamedColor> for Colors {
    type Output = Option<Rgb>;

    #[inline]
    fn index(&self, index: NamedColor) -> &Self::Output {
        &self.0[index as usize]
    }
}

impl IndexMut<NamedColor> for Colors {
    #[inline]
    fn index_mut(&mut self, index: NamedColor) -> &mut Self::Output {
        &mut self.0[index as usize]
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
