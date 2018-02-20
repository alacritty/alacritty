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
use serde_yaml;
use serde::{de, Deserialize};

use config::{Delta, failure_default};
use font::{Size, FontKey, Slant};

// Global and local font configuration
#[derive(Deserialize, Debug)]
pub struct FontConfiguration {
    #[serde(default, deserialize_with="failure_default")]
    options: GlobalOptions,
    #[serde(default, deserialize_with="deserialize_font_collection")]
    fonts: Vec<Font>,
}

impl FontConfiguration {
    pub fn font_by_char(&self, c: char, weight: Slant) -> FontKey {
        for font in fonts {
            let options = font.options();

            // Skip font if font slant does not match requested slant
            if options.unwrap_or(self.options as Options).style.unwrap_or(self.options.style) != weight {
                continue;
            }

            let is_match = match options {
                Some(options) => {
                    for range in options.ranges {
                        if range.start < c && range.end > c {
                            return true;
                        }
                    }
                    false
                },
                None => {
                    true
                },
            };

            if is_match {
                // TODO: Check if this font contains the char
            }
        }
    }
}

impl Default for FontConfiguration {
    fn default() -> Self {
        Self {
            fonts: vec!(Font::default()),
            ..Default::default()
        }
    }
}

// Information about a font
#[derive(Deserialize, Debug)]
pub struct Font {
    family: String,
    #[serde(deserialize_with="failure_default")]
    options: Option<Options>,
}

// Default font config in case of failure or missing config
impl Default for Font {
    #[cfg(target_os = "macos")]
    fn default() -> Self {
        Font {
            family: "Menlo".into(),
            options: None,
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn default() -> Self {
        Font {
            family: "monospace".into(),
            options: None,
        }
    }
}

// Options for a font
#[derive(Deserialize, Debug)]
pub struct Options {
    #[serde(deserialize_with="deserialize_size")]
    size: Option<Size>,
    #[serde(deserialize_with="failure_default")]
    thin_strokes: Option<bool>,
    #[serde(deserialize_with="failure_default")]
    antialias: Option<AntiAlias>,
    #[serde(deserialize_with="failure_default")]
    hinting: Option<bool>,
    #[serde(deserialize_with="failure_default")]
    style: Option<String>,
    #[serde(deserialize_with="failure_default")]
    offset: Option<Delta>,
    #[serde(deserialize_with="failure_default")]
    ranges: Vec<FontRange>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            size: None,
            thin_strokes: None,
            antialias: None,
            hinting: None,
            style: None,
            offset: None,
            ranges: Vec::new(),
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct GlobalOptions(Options);
impl Default for GlobalOptions {
    fn default() -> Self {
        GlobalOptions(Options {
            size: Some(Size::new(12.0)),
            thin_strokes: Some(true),
            antialias: Some(AntiAlias::LCD),
            hinting: Some(true),
            style: Some("normal".into()),
            offset: Some(Delta::default()),
            ranges: Vec::new(),
        })
    }
}

// AntiAliasing settings for fonts
#[derive(Deserialize, Debug)]
pub enum AntiAlias {
    LCD,
    LCDV,
    GRAY,
    DISABLED,
}

// Range for which a specific font should be used
#[derive(Deserialize, Debug)]
pub struct FontRange {
    #[serde(deserialize_with="failure_default")]
    start: char,
    #[serde(deserialize_with="failure_default")]
    end: char,
}

// Deserialize the font vector
fn deserialize_font_collection<'a, D>(deserializer: D)
    -> ::std::result::Result<Vec<Font>, D::Error>
    where D: de::Deserializer<'a>,
{
    // Deserialize vector as generic yaml value
    let mut value = match serde_yaml::Value::deserialize(deserializer) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("problem with config: {}; Using default fonts", err);
            return Ok(vec!(Font::default()));
        },
    };

    // Get value as sequence
    let sequence = match value.as_sequence_mut() {
        Some(sequence) => sequence,
        None => return Ok(vec!(Font::default())),
    };

    // Deserialize each element in the sequence
    let mut font_collection = Vec::new();
    for i in 0..sequence.len() {
        match Font::deserialize(sequence.remove(i)) {
            Ok(font) => font_collection.push(font),
            // TODO: Print line or something like that?
            Err(err) => eprintln!("problem with config: Malformed font; Skipping"),
        }
    }

    // Return defaults if collection contains no font
    if font_collection.is_empty() {
        Ok(vec!(Font::default()))
    } else {
        Ok(font_collection)
    }
}

// Deserialize font size
fn deserialize_size<'a, D>(deserializer: D)
    -> ::std::result::Result<Option<Size>, D::Error>
    where D: de::Deserializer<'a>,
{
    match f32::deserialize(deserializer) {
        Ok(value) => Ok(Some(Size::new(value))),
        _ => {
            Ok(None)
        },
    }
}
