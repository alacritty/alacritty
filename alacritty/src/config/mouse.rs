use std::time::Duration;

use glutin::event::ModifiersState;
use log::error;
use serde::{Deserialize, Deserializer};

use alacritty_terminal::config::{failure_default, LOG_TARGET_CONFIG};

use crate::config::bindings::{CommandWrapper, ModsWrapper};

#[serde(default)]
#[derive(Default, Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Mouse {
    #[serde(deserialize_with = "failure_default")]
    pub double_click: ClickHandler,
    #[serde(deserialize_with = "failure_default")]
    pub triple_click: ClickHandler,
    #[serde(deserialize_with = "failure_default")]
    pub hide_when_typing: bool,
    #[serde(deserialize_with = "failure_default")]
    pub url: Url,
}

#[serde(default)]
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Url {
    // Program for opening links
    #[serde(deserialize_with = "deserialize_launcher")]
    pub launcher: Option<CommandWrapper>,

    // Modifier used to open links
    #[serde(deserialize_with = "failure_default")]
    modifiers: ModsWrapper,
}

impl Url {
    pub fn mods(&self) -> ModifiersState {
        self.modifiers.into_inner()
    }
}

fn deserialize_launcher<'a, D>(
    deserializer: D,
) -> ::std::result::Result<Option<CommandWrapper>, D::Error>
where
    D: Deserializer<'a>,
{
    let default = Url::default().launcher;

    // Deserialize to generic value
    let val = serde_yaml::Value::deserialize(deserializer)?;

    // Accept `None` to disable the launcher
    if val.as_str().filter(|v| v.to_lowercase() == "none").is_some() {
        return Ok(None);
    }

    match <Option<CommandWrapper>>::deserialize(val) {
        Ok(launcher) => Ok(launcher),
        Err(err) => {
            error!(
                target: LOG_TARGET_CONFIG,
                "Problem with config: {}; using {}",
                err,
                default.clone().unwrap().program()
            );
            Ok(default)
        },
    }
}

impl Default for Url {
    fn default() -> Url {
        Url {
            #[cfg(not(any(target_os = "macos", windows)))]
            launcher: Some(CommandWrapper::Just(String::from("xdg-open"))),
            #[cfg(target_os = "macos")]
            launcher: Some(CommandWrapper::Just(String::from("open"))),
            #[cfg(windows)]
            launcher: Some(CommandWrapper::Just(String::from("explorer"))),
            modifiers: Default::default(),
        }
    }
}

#[serde(default)]
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ClickHandler {
    #[serde(deserialize_with = "deserialize_duration_ms")]
    pub threshold: Duration,
}

impl Default for ClickHandler {
    fn default() -> Self {
        ClickHandler { threshold: default_threshold_ms() }
    }
}

fn default_threshold_ms() -> Duration {
    Duration::from_millis(300)
}

fn deserialize_duration_ms<'a, D>(deserializer: D) -> ::std::result::Result<Duration, D::Error>
where
    D: Deserializer<'a>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    match u64::deserialize(value) {
        Ok(threshold_ms) => Ok(Duration::from_millis(threshold_ms)),
        Err(err) => {
            error!(target: LOG_TARGET_CONFIG, "Problem with config: {}; using default value", err);
            Ok(default_threshold_ms())
        },
    }
}
