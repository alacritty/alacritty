use std::time::Duration;

use log::error;
use serde::{Deserialize, Deserializer};
use serde_yaml::Value;

use crate::config::{failure_default, Program, LOG_TARGET_CONFIG};
use crate::term::color::Rgb;

const DEFAULT_BELL_COLOR: Rgb = Rgb { r: 255, g: 255, b: 255 };

#[serde(default)]
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct BellConfig {
    /// Visual bell animation function.
    #[serde(deserialize_with = "failure_default")]
    pub animation: BellAnimation,

    /// Visual bell duration in milliseconds.
    #[serde(deserialize_with = "failure_default")]
    duration: u16,

    /// Visual bell flash color.
    #[serde(deserialize_with = "deserialize_bell_color")]
    pub color: Rgb,

    /// Command to run on bell.
    #[serde(deserialize_with = "deserialize_bell_command")]
    pub command: Option<Program>,
}

impl Default for BellConfig {
    fn default() -> Self {
        Self {
            animation: Default::default(),
            duration: Default::default(),
            command: Default::default(),
            color: DEFAULT_BELL_COLOR,
        }
    }
}

impl BellConfig {
    /// Visual bell duration in milliseconds.
    #[inline]
    pub fn duration(&self) -> Duration {
        Duration::from_millis(u64::from(self.duration))
    }
}

fn deserialize_bell_color<'a, D>(deserializer: D) -> Result<Rgb, D::Error>
where
    D: Deserializer<'a>,
{
    let value = Value::deserialize(deserializer)?;
    match Rgb::deserialize(value) {
        Ok(value) => Ok(value),
        Err(err) => {
            error!(
                target: LOG_TARGET_CONFIG,
                "Problem with config: {}, using default color value {}", err, DEFAULT_BELL_COLOR
            );

            Ok(DEFAULT_BELL_COLOR)
        },
    }
}

fn deserialize_bell_command<'a, D>(deserializer: D) -> Result<Option<Program>, D::Error>
where
    D: Deserializer<'a>,
{
    // Deserialize to generic value.
    let val = Value::deserialize(deserializer)?;

    // Accept `None` to disable the bell command.
    if val.as_str().filter(|v| v.to_lowercase() == "none").is_some() {
        return Ok(None);
    }

    match Program::deserialize(val) {
        Ok(command) => Ok(Some(command)),
        Err(err) => {
            error!(target: LOG_TARGET_CONFIG, "Problem with config: {}; ignoring field", err);
            Ok(None)
        },
    }
}

/// `VisualBellAnimations` are modeled after a subset of CSS transitions and Robert
/// Penner's Easing Functions.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
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
    EaseOutExpo,
    // Penner animation.
    EaseOutCirc,
    // Penner animation.
    Linear,
}

impl Default for BellAnimation {
    fn default() -> Self {
        BellAnimation::EaseOutExpo
    }
}
