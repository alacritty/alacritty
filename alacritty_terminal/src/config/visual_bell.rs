use std::time::Duration;

use serde::Deserialize;

use crate::config::failure_default;
use crate::term::color::Rgb;

#[serde(default)]
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct VisualBellConfig {
    /// Visual bell animation function.
    #[serde(deserialize_with = "failure_default")]
    pub animation: VisualBellAnimation,

    /// Visual bell duration in milliseconds.
    #[serde(deserialize_with = "failure_default")]
    pub duration: u16,

    /// Visual bell flash color.
    #[serde(deserialize_with = "failure_default")]
    pub color: Rgb,
}

impl Default for VisualBellConfig {
    fn default() -> VisualBellConfig {
        VisualBellConfig {
            animation: Default::default(),
            duration: Default::default(),
            color: default_visual_bell_color(),
        }
    }
}

impl VisualBellConfig {
    /// Visual bell duration in milliseconds.
    #[inline]
    pub fn duration(&self) -> Duration {
        Duration::from_millis(u64::from(self.duration))
    }
}

/// `VisualBellAnimations` are modeled after a subset of CSS transitions and Robert
/// Penner's Easing Functions.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum VisualBellAnimation {
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
    EaseOutExpo,
    // Penner animation.
    EaseOutCirc,
    // Penner animation.
    Linear,
}

impl Default for VisualBellAnimation {
    fn default() -> Self {
        VisualBellAnimation::EaseOutExpo
    }
}

fn default_visual_bell_color() -> Rgb {
    Rgb { r: 255, g: 255, b: 255 }
}
