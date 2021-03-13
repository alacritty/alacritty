use std::time::Duration;

use glutin::event::ModifiersState;

use alacritty_config_derive::ConfigDeserialize;
use alacritty_terminal::config::Program;

use crate::config::bindings::ModsWrapper;

#[derive(ConfigDeserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct Mouse {
    pub double_click: ClickHandler,
    pub triple_click: ClickHandler,
    pub hide_when_typing: bool,
    pub url: Url,
}

#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Url {
    /// Program for opening links.
    pub launcher: Option<Program>,

    /// Modifier used to open links.
    modifiers: ModsWrapper,
}

impl Url {
    pub fn mods(&self) -> ModifiersState {
        self.modifiers.into_inner()
    }
}

impl Default for Url {
    fn default() -> Url {
        Url {
            #[cfg(not(any(target_os = "macos", windows)))]
            launcher: Some(Program::Just(String::from("xdg-open"))),
            #[cfg(target_os = "macos")]
            launcher: Some(Program::Just(String::from("open"))),
            #[cfg(windows)]
            launcher: Some(Program::WithArgs {
                program: String::from("cmd"),
                args: vec!["/c".to_string(), "start".to_string(), "".to_string()],
            }),
            modifiers: Default::default(),
        }
    }
}

#[derive(ConfigDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ClickHandler {
    threshold: u16,
}

impl Default for ClickHandler {
    fn default() -> Self {
        Self { threshold: 300 }
    }
}

impl ClickHandler {
    pub fn threshold(&self) -> Duration {
        Duration::from_millis(self.threshold as u64)
    }
}
