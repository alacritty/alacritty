use std::time::Duration;

use crate::config::failure_default;
use crate::term::color::Rgb;

#[serde(default)]
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct BellConfig {
    /// Visual bell animation function
    #[serde(deserialize_with = "failure_default")]
    pub animation: VisualBellAnimation,

    /// Visual bell duration in milliseconds
    #[serde(deserialize_with = "failure_default")]
    pub duration: u16,

    /// Visual bell flash color
    #[serde(deserialize_with = "failure_default")]
    pub color: Rgb,

    #[serde(deserialize_with = "failure_default")]
    pub command: Option<CommandWrapper>,
}

impl Default for BellConfig {
    fn default() -> BellConfig {
        BellConfig {
            animation: Default::default(),
            duration: Default::default(),
            color: default_visual_bell_color(),
            bell_command: None,
        }
    }
}

impl BellConfig {
    /// Visual bell duration in milliseconds
    #[inline]
    pub fn duration(&self) -> Duration {
        Duration::from_millis(u64::from(self.duration))
    }
}

/// `VisualBellAnimations` are modeled after a subset of CSS transitions and Robert
/// Penner's Easing Functions.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum VisualBellAnimation {
    Ease,         // CSS
    EaseOut,      // CSS
    EaseOutSine,  // Penner
    EaseOutQuad,  // Penner
    EaseOutCubic, // Penner
    EaseOutQuart, // Penner
    EaseOutQuint, // Penner
    EaseOutExpo,  // Penner
    EaseOutCirc,  // Penner
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
