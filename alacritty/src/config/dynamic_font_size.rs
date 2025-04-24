use std::fmt;

use crossfont::Size as FontSize;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer};
use winit::dpi::PhysicalSize;
use alacritty_config_derive::{ConfigDeserialize, SerdeReplace};

/// Dynamic font size config.
///
/// TODO
#[derive(ConfigDeserialize, Debug, Clone, Default, PartialEq)]
pub struct DynamicFontSize {
    small_font: Option<DynamicFontSizeEntry>,
    large_font: Option<DynamicFontSizeEntry>,
}

impl DynamicFontSize {
    pub fn determine_font_size(self, normal: FontSize, window_size: &PhysicalSize<u32>) -> FontSize {
        match self.determine_dynamic_font_scaling(window_size) {
            None => { normal }
            Some(dynamic_font_scale) => { normal.scale(dynamic_font_scale) }
        }
    }

    /// If the given `window_size` matches a `DynamicFontSizeEntry`, return the configured scaling.
    pub fn determine_dynamic_font_scaling(self, window_size: &PhysicalSize<u32>) -> Option<f32> {
        // Test for large font if configured.
        if let Some(dynamic_font) = self.large_font {
            if window_size.width > dynamic_font.at.width
                && window_size.height > dynamic_font.at.height {
                return Some(dynamic_font.scale);
            }
        }
        // Test for small font if configured.
        if let Some(dynamic_font) = self.small_font {
            if window_size.width < dynamic_font.at.width
                || window_size.height < dynamic_font.at.height {
                return Some(dynamic_font.scale);
            }
        }
        // No configured dynamic font size matches the current window size.
        None
    }
}

#[derive(ConfigDeserialize, Debug, Default, Clone, PartialEq)]
struct DynamicFontSizeEntry {
    at: WindowSize,
    scale: f32
}

/// Represents TODO comments
#[derive(ConfigDeserialize, Debug, Default, Clone, Copy, PartialEq, Eq)]
struct WindowSize {
    width: u32,
    height: u32,
}
