use std::time::Duration;

use alacritty_config_derive::ConfigDeserialize;

use alacritty_terminal::config::{Percentage, Program};
use alacritty_terminal::term::color::Rgb;

#[derive(ConfigDeserialize, Clone, Debug, PartialEq)]
pub struct BellConfig {
    /// Visual bell animation function.
    pub animation: BellAnimation,

    /// Command to run on bell.
    pub command: Option<Program>,

    /// Visual bell flash color.
    pub color: Rgb,

    /// Maximum opacity of the flash color, from 0.0 to 1.0.
    pub max_intensity: Percentage,

    /// Visual bell duration in milliseconds.
    duration: u16,
}

impl Default for BellConfig {
    fn default() -> Self {
        Self {
            color: Rgb::new(255, 255, 255),
            animation: Default::default(),
            command: Default::default(),
            duration: Default::default(),
            max_intensity: Default::default(),
        }
    }
}

impl BellConfig {
    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.duration as u64)
    }
}

/// `VisualBellAnimations` are modeled after a subset of CSS transitions and Robert
/// Penner's Easing Functions.
#[derive(ConfigDeserialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BellAnimation {
    // CSS animation.
    Ease,
    // CSS animation.
    EaseOut,
    // Penner animation.
    EaseOutSine,
    // Penner animation.
    EaseOutQuad,
    // Penner animation.
    EaseOutCubic,
    // Penner animation.
    EaseOutQuart,
    // Penner animation.
    EaseOutQuint,
    // Penner animation.
    #[default]
    EaseOutExpo,
    // Penner animation.
    EaseOutCirc,
    // Penner animation.
    Linear,
}
