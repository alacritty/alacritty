use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer, Serialize};

use alacritty_config_derive::{ConfigDeserialize, SerdeReplace};

/// Maximum scrollback amount configurable.
pub const MAX_SCROLLBACK_LINES: u32 = 100_000;

/// Struct for scrolling related settings.
#[derive(ConfigDeserialize, Serialize, Copy, Clone, Debug, PartialEq, Eq)]
pub struct Scrolling {
    pub multiplier: u8,

    history: ScrollingHistory,
}

impl Default for Scrolling {
    fn default() -> Self {
        Self { multiplier: 3, history: Default::default() }
    }
}

impl Scrolling {
    pub fn history(self) -> u32 {
        self.history.0
    }
}

#[derive(SerdeReplace, Serialize, Copy, Clone, Debug, PartialEq, Eq)]
struct ScrollingHistory(u32);

impl Default for ScrollingHistory {
    fn default() -> Self {
        Self(10_000)
    }
}

impl<'de> Deserialize<'de> for ScrollingHistory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let lines = u32::deserialize(deserializer)?;

        if lines > MAX_SCROLLBACK_LINES {
            Err(SerdeError::custom(format!(
                "exceeded maximum scrolling history ({lines}/{MAX_SCROLLBACK_LINES})"
            )))
        } else {
            Ok(Self(lines))
        }
    }
}
