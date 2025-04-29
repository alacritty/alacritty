use std::fmt;

use crossfont::Size as FontSize;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer};
use winit::dpi::PhysicalSize;
use alacritty_config_derive::{ConfigDeserialize, SerdeReplace};

/// Dynamic font size config.
///
/// The config contains two optional `DynamicFontSizeEntry`s, which specify
/// how much the font should be scaled if the window is larger or smaller than specified.
#[derive(ConfigDeserialize, Debug, Clone, Default, PartialEq)]
pub struct DynamicFontSize {
    small_font: Option<DynamicFontSizeEntry>,
    large_font: Option<DynamicFontSizeEntry>,
}

impl DynamicFontSize {
    /// Determines the `FontSize` to be displayed with the current `window_size`, based on the user's
    /// dynamic font size configuration.
    pub fn determine_font_size(self, normal: FontSize, window_size: &PhysicalSize<u32>) -> FontSize {
        self.determine_dynamic_font_scaling(window_size).map_or(normal, |s| normal.scale(s))
    }

    /// If the given `window_size` matches a `DynamicFontSizeEntry`, return the configured scaling.
    pub fn determine_dynamic_font_scaling(self, window_size: &PhysicalSize<u32>) -> Option<f32> {
        // Test for large font if configured.
        self.large_font.and_then(|dynamic_font| {
            (window_size.width > dynamic_font.at.width
                && window_size.height > dynamic_font.at.height
            ).then_some(dynamic_font.scale)
        // Test for small font if configured.
        }).xor(self.small_font.and_then(|dynamic_font| {
            (window_size.width < dynamic_font.at.width
                || window_size.height < dynamic_font.at.height
            ).then_some(dynamic_font.scale)
        }))
        // No configured dynamic font size matches the current window size if None.
    }
}

/// Defines how much the font size should be scaled, when the window size goes beyond the specified
/// `WindowSize`. At this scope, the entry is agnostic of whether it is used for defining the small
/// or large font, and thus whether 'beyond' means smaller or larger than the specified `WindowSize`.
#[derive(ConfigDeserialize, Debug, Default, Clone, PartialEq)]
struct DynamicFontSizeEntry {
    at: WindowSize,
    scale: f32
}

/// Represents the size of the terminal window, as minimalistic as possible such that it can be
/// parsed from the config.
#[derive(ConfigDeserialize, Debug, Default, Clone, Copy, PartialEq, Eq)]
struct WindowSize {
    width: u32,
    height: u32,
}
