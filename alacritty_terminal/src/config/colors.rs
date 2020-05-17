use log::error;
use serde::{Deserialize, Deserializer};

use crate::config::{failure_default, LOG_TARGET_CONFIG};
use crate::term::color::Rgb;

#[serde(default)]
#[derive(Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Colors {
    #[serde(deserialize_with = "failure_default")]
    pub primary: PrimaryColors,
    #[serde(deserialize_with = "failure_default")]
    pub cursor: CursorColors,
    #[serde(deserialize_with = "failure_default")]
    pub vi_mode_cursor: CursorColors,
    #[serde(deserialize_with = "failure_default")]
    pub selection: SelectionColors,
    #[serde(deserialize_with = "failure_default")]
    normal: NormalColors,
    #[serde(deserialize_with = "failure_default")]
    bright: BrightColors,
    #[serde(deserialize_with = "failure_default")]
    pub dim: Option<AnsiColors>,
    #[serde(deserialize_with = "failure_default")]
    pub indexed_colors: Vec<IndexedColor>,
}

impl Colors {
    pub fn normal(&self) -> &AnsiColors {
        &self.normal.0
    }

    pub fn bright(&self) -> &AnsiColors {
        &self.bright.0
    }
}

#[serde(default)]
#[derive(Deserialize, Clone, Default, Debug, PartialEq, Eq)]
pub struct IndexedColor {
    #[serde(deserialize_with = "deserialize_color_index")]
    pub index: u8,
    #[serde(deserialize_with = "failure_default")]
    pub color: Rgb,
}

fn deserialize_color_index<'a, D>(deserializer: D) -> ::std::result::Result<u8, D::Error>
where
    D: Deserializer<'a>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    match u8::deserialize(value) {
        Ok(index) => {
            if index < 16 {
                error!(
                    target: LOG_TARGET_CONFIG,
                    "Problem with config: indexed_color's index is {}, but a value bigger than 15 \
                     was expected; ignoring setting",
                    index
                );

                // Return value out of range to ignore this color.
                Ok(0)
            } else {
                Ok(index)
            }
        },
        Err(err) => {
            error!(target: LOG_TARGET_CONFIG, "Problem with config: {}; ignoring setting", err);

            // Return value out of range to ignore this color.
            Ok(0)
        },
    }
}

#[serde(default)]
#[derive(Deserialize, Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct CursorColors {
    #[serde(deserialize_with = "failure_default")]
    pub text: Option<Rgb>,
    #[serde(deserialize_with = "failure_default")]
    pub cursor: Option<Rgb>,
}

#[serde(default)]
#[derive(Deserialize, Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct SelectionColors {
    #[serde(deserialize_with = "failure_default")]
    pub text: Option<Rgb>,
    #[serde(deserialize_with = "failure_default")]
    pub background: Option<Rgb>,
}

#[serde(default)]
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PrimaryColors {
    #[serde(default = "default_background", deserialize_with = "failure_default")]
    pub background: Rgb,
    #[serde(default = "default_foreground", deserialize_with = "failure_default")]
    pub foreground: Rgb,
    #[serde(deserialize_with = "failure_default")]
    pub bright_foreground: Option<Rgb>,
    #[serde(deserialize_with = "failure_default")]
    pub dim_foreground: Option<Rgb>,
}

impl Default for PrimaryColors {
    fn default() -> Self {
        PrimaryColors {
            background: default_background(),
            foreground: default_foreground(),
            bright_foreground: Default::default(),
            dim_foreground: Default::default(),
        }
    }
}

fn default_background() -> Rgb {
    Rgb { r: 0x1d, g: 0x1f, b: 0x21 }
}

fn default_foreground() -> Rgb {
    Rgb { r: 0xc5, g: 0xc8, b: 0xc6 }
}

/// The 8-colors sections of config.
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AnsiColors {
    #[serde(deserialize_with = "failure_default")]
    pub black: Rgb,
    #[serde(deserialize_with = "failure_default")]
    pub red: Rgb,
    #[serde(deserialize_with = "failure_default")]
    pub green: Rgb,
    #[serde(deserialize_with = "failure_default")]
    pub yellow: Rgb,
    #[serde(deserialize_with = "failure_default")]
    pub blue: Rgb,
    #[serde(deserialize_with = "failure_default")]
    pub magenta: Rgb,
    #[serde(deserialize_with = "failure_default")]
    pub cyan: Rgb,
    #[serde(deserialize_with = "failure_default")]
    pub white: Rgb,
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
struct NormalColors(AnsiColors);

impl Default for NormalColors {
    fn default() -> Self {
        NormalColors(AnsiColors {
            black: Rgb { r: 0x1d, g: 0x1f, b: 0x21 },
            red: Rgb { r: 0xcc, g: 0x66, b: 0x66 },
            green: Rgb { r: 0xb5, g: 0xbd, b: 0x68 },
            yellow: Rgb { r: 0xf0, g: 0xc6, b: 0x74 },
            blue: Rgb { r: 0x81, g: 0xa2, b: 0xbe },
            magenta: Rgb { r: 0xb2, g: 0x94, b: 0xbb },
            cyan: Rgb { r: 0x8a, g: 0xbe, b: 0xb7 },
            white: Rgb { r: 0xc5, g: 0xc8, b: 0xc6 },
        })
    }
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
struct BrightColors(AnsiColors);

impl Default for BrightColors {
    fn default() -> Self {
        BrightColors(AnsiColors {
            black: Rgb { r: 0x66, g: 0x66, b: 0x66 },
            red: Rgb { r: 0xd5, g: 0x4e, b: 0x53 },
            green: Rgb { r: 0xb9, g: 0xca, b: 0x4a },
            yellow: Rgb { r: 0xe7, g: 0xc5, b: 0x47 },
            blue: Rgb { r: 0x7a, g: 0xa6, b: 0xda },
            magenta: Rgb { r: 0xc3, g: 0x97, b: 0xd8 },
            cyan: Rgb { r: 0x70, g: 0xc0, b: 0xb1 },
            white: Rgb { r: 0xea, g: 0xea, b: 0xea },
        })
    }
}
