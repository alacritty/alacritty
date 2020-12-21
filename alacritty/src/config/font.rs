use std::fmt;

use crossfont::Size as FontSize;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer};

use alacritty_config_derive::ConfigDeserialize;

use crate::config::ui_config::Delta;

/// Font config.
///
/// Defaults are provided at the level of this struct per platform, but not per
/// field in this struct. It might be nice in the future to have defaults for
/// each value independently. Alternatively, maybe erroring when the user
/// doesn't provide complete config is Ok.
#[derive(ConfigDeserialize, Default, Debug, Clone, PartialEq, Eq)]
pub struct Font {
    /// Extra spacing per character.
    pub offset: Delta<i8>,

    /// Glyph offset within character cell.
    pub glyph_offset: Delta<i8>,

    pub use_thin_strokes: bool,

    /// Normal font face.
    normal: FontDescription,

    /// Bold font face.
    bold: SecondaryFontDescription,

    /// Italic font face.
    italic: SecondaryFontDescription,

    /// Bold italic font face.
    bold_italic: SecondaryFontDescription,

    /// Font size in points.
    size: Size,
}

impl Font {
    /// Get a font clone with a size modification.
    pub fn with_size(self, size: FontSize) -> Font {
        Font { size: Size(size), ..self }
    }

    #[inline]
    pub fn size(&self) -> FontSize {
        self.size.0
    }

    /// Get normal font description.
    pub fn normal(&self) -> &FontDescription {
        &self.normal
    }

    /// Get bold font description.
    pub fn bold(&self) -> FontDescription {
        self.bold.desc(&self.normal)
    }

    /// Get italic font description.
    pub fn italic(&self) -> FontDescription {
        self.italic.desc(&self.normal)
    }

    /// Get bold italic font description.
    pub fn bold_italic(&self) -> FontDescription {
        self.bold_italic.desc(&self.normal)
    }
}

/// Description of the normal font.
#[derive(ConfigDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct FontDescription {
    pub family: String,
    pub style: Option<String>,
}

impl Default for FontDescription {
    fn default() -> FontDescription {
        FontDescription {
            #[cfg(not(any(target_os = "macos", windows)))]
            family: "monospace".into(),
            #[cfg(target_os = "macos")]
            family: "Menlo".into(),
            #[cfg(windows)]
            family: "Consolas".into(),
            style: None,
        }
    }
}

/// Description of the italic and bold font.
#[derive(ConfigDeserialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct SecondaryFontDescription {
    family: Option<String>,
    style: Option<String>,
}

impl SecondaryFontDescription {
    pub fn desc(&self, fallback: &FontDescription) -> FontDescription {
        FontDescription {
            family: self.family.clone().unwrap_or_else(|| fallback.family.clone()),
            style: self.style.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Size(FontSize);

impl Default for Size {
    fn default() -> Self {
        Self(FontSize::new(11.))
    }
}

impl<'de> Deserialize<'de> for Size {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct NumVisitor;
        impl<'v> Visitor<'v> for NumVisitor {
            type Value = Size;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("f64 or u64")
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Ok(Size(FontSize::new(value as f32)))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(Size(FontSize::new(value as f32)))
            }
        }

        deserializer.deserialize_any(NumVisitor)
    }
}
