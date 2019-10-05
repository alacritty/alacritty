use log::error;
use serde::{Deserialize, Deserializer};

use crate::config::{failure_default, LOG_TARGET_CONFIG, MAX_SCROLLBACK_LINES};

/// Struct for scrolling related settings
#[serde(default)]
#[derive(Deserialize, Copy, Clone, Default, Debug, PartialEq, Eq)]
pub struct Scrolling {
    #[serde(deserialize_with = "failure_default")]
    history: ScrollingHistory,
    #[serde(deserialize_with = "failure_default")]
    multiplier: ScrollingMultiplier,
    #[serde(deserialize_with = "failure_default")]
    faux_multiplier: ScrollingMultiplier,
    #[serde(deserialize_with = "failure_default")]
    pub auto_scroll: bool,
}

impl Scrolling {
    pub fn history(self) -> u32 {
        self.history.0
    }

    pub fn multiplier(self) -> u8 {
        self.multiplier.0
    }

    pub fn faux_multiplier(self) -> u8 {
        self.faux_multiplier.0
    }

    // Update the history size, used in ref tests
    pub fn set_history(&mut self, history: u32) {
        self.history = ScrollingHistory(history);
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
struct ScrollingMultiplier(u8);

impl Default for ScrollingMultiplier {
    fn default() -> Self {
        ScrollingMultiplier(3)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ScrollingHistory(u32);

impl Default for ScrollingHistory {
    fn default() -> Self {
        ScrollingHistory(10_000)
    }
}

impl<'de> Deserialize<'de> for ScrollingHistory {
    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;
        match u32::deserialize(value) {
            Ok(lines) => {
                if lines > MAX_SCROLLBACK_LINES {
                    error!(
                        target: LOG_TARGET_CONFIG,
                        "Problem with config: scrollback size is {}, but expected a maximum of \
                         {}; using {1} instead",
                        lines,
                        MAX_SCROLLBACK_LINES,
                    );
                    Ok(ScrollingHistory(MAX_SCROLLBACK_LINES))
                } else {
                    Ok(ScrollingHistory(lines))
                }
            },
            Err(err) => {
                error!(
                    target: LOG_TARGET_CONFIG,
                    "Problem with config: {}; using default value", err
                );
                Ok(Default::default())
            },
        }
    }
}
