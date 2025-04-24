use std::fmt;

use crossfont::Size as FontSize;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer};
use alacritty_config_derive::{ConfigDeserialize, SerdeReplace};
use crate::config::ui_config::Delta;

/// Font config.
///
/// Defaults are provided at the level of this struct per platform, but not per
/// field in this struct. It might be nice in the future to have defaults for
/// each value independently. Alternatively, maybe erroring when the user
/// doesn't provide complete config is Ok.
#[derive(ConfigDeserialize, Debug, Clone, PartialEq, Eq)]
pub struct Font {
    /// Extra spacing per character.
    pub offset: Delta<i8>,

    /// Glyph offset within character cell.
    pub glyph_offset: Delta<i8>,

    #[config(removed = "set the AppleFontSmoothing user default instead")]
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

    /// Parameters which dynamically update the font to a specified font
    dynamic_size: DynamicFontDescription, // TODO move to other config field

    /// Whether to use the built-in font for box drawing characters.
    pub builtin_box_drawing: bool,
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

    /// Get dynamic font description.
    pub fn dynamic_font(&self) -> &DynamicFontDescription { &self.dynamic_size }
}

impl Default for Font {
    fn default() -> Font {
        Self {
            builtin_box_drawing: true,
            glyph_offset: Default::default(),
            use_thin_strokes: Default::default(),
            bold_italic: Default::default(),
            italic: Default::default(),
            offset: Default::default(),
            normal: Default::default(),
            bold: Default::default(),
            dynamic_size: Default::default(),
            size: Default::default(),
        }
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

// TODO set proper defaults
#[derive(ConfigDeserialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct DynamicFontThreshold {
    pub window_width: u32,
    pub window_height: u32,
}

impl DynamicFontThreshold {
    pub fn new(window_width: u32, window_height: u32) -> DynamicFontThreshold {
        DynamicFontThreshold { window_width, window_height }
    }
}

/// Dynamic font threshold description.
#[derive(ConfigDeserialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct DynamicFontDescription {
    pub small_at: Option<DynamicFontThreshold>,
    pub large_at: Option<DynamicFontThreshold>,
    pub small_scale_factor: Option<u32>,
    pub large_scale_factor: Option<u32>,
}

// TODO hide public fields with this
// impl DynamicFontDescription {
//     pub fn determine_current_font_size<T>(&self, window_size: PhysicalSize<T>, font_size: &FontSize){
//         if window_size.width > self.large_at.width && window_size.height > self.large_at.height { // TODO move to another function
//             // Use enlarged configured font.
//             let large_font_window_size = FontSize::new(dynamic_font.large_scale_factor.unwrap());
//             let large_font = config_font.with_window_size(large_font_window_size);
//             display_update_pending.set_font(large_font);
//         }
//         else if window_size.width < small_at.width && window_size.height < small_at.height {
//             // Use shrunk configured font.
//             let small_font_window_size = FontSize::new(dynamic_font.small_scale_factor.unwrap());
//             let small_font = config_font.with_window_size(small_font_window_size);
//             display_update_pending.set_font(small_font);
//         }
//         else {
//             // Use normal-window_sized configured font.
//             display_update_pending.set_font(config_font);
//         }
//     }
// }

#[derive(SerdeReplace, Debug, Clone, PartialEq, Eq)]
struct Size(FontSize);

impl Default for Size {
    fn default() -> Self {
        Self(FontSize::new(11.25))
    }
}

impl<'de> Deserialize<'de> for Size {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct NumVisitor;
        impl Visitor<'_> for NumVisitor {
            type Value = Size;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("f64 or i64")
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Ok(Size(FontSize::new(value as f32)))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(Size(FontSize::new(value as f32)))
            }
        }

        deserializer.deserialize_any(NumVisitor)
    }
}
