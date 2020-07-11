use std::env;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io;
use std::path::PathBuf;

#[cfg(windows)]
use dirs;
use log::{error, warn};

use alacritty_terminal::config::{Config as TermConfig, LOG_TARGET_CONFIG};

mod bindings;
pub mod debug;
pub mod font;
pub mod monitor;
mod mouse;
pub mod ui_config;
pub mod window;

pub use crate::config::bindings::{Action, Binding, Key, ViAction};
#[cfg(test)]
pub use crate::config::mouse::{ClickHandler, Mouse};
use crate::config::ui_config::UIConfig;

pub type Config = TermConfig<UIConfig>;

/// Result from config loading.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors occurring during config loading.
#[derive(Debug)]
pub enum Error {
    /// Config file not found.
    NotFound,

    /// Couldn't read $HOME environment variable.
    ReadingEnvHome(env::VarError),

    /// io error reading file.
    Io(io::Error),

    /// Not valid yaml or missing parameters.
    Yaml(serde_yaml::Error),
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::NotFound => None,
            Error::ReadingEnvHome(err) => err.source(),
            Error::Io(err) => err.source(),
            Error::Yaml(err) => err.source(),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound => write!(f, "Unable to locate config file"),
            Error::ReadingEnvHome(err) => {
                write!(f, "Unable to read $HOME environment variable: {}", err)
            },
            Error::Io(err) => write!(f, "Error reading config file: {}", err),
            Error::Yaml(err) => write!(f, "Problem with config: {}", err),
        }
    }
}

impl From<env::VarError> for Error {
    fn from(val: env::VarError) -> Self {
        Error::ReadingEnvHome(val)
    }
}

impl From<io::Error> for Error {
    fn from(val: io::Error) -> Self {
        if val.kind() == io::ErrorKind::NotFound {
            Error::NotFound
        } else {
            Error::Io(val)
        }
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(val: serde_yaml::Error) -> Self {
        Error::Yaml(val)
    }
}

/// Get the location of the first found default config file paths
/// according to the following order:
///
/// 1. $XDG_CONFIG_HOME/alacritty/alacritty.yml
/// 2. $XDG_CONFIG_HOME/alacritty.yml
/// 3. $HOME/.config/alacritty/alacritty.yml
/// 4. $HOME/.alacritty.yml
#[cfg(not(windows))]
pub fn installed_config() -> Option<PathBuf> {
    // Try using XDG location by default.
    xdg::BaseDirectories::with_prefix("alacritty")
        .ok()
        .and_then(|xdg| xdg.find_config_file("alacritty.yml"))
        .or_else(|| {
            xdg::BaseDirectories::new()
                .ok()
                .and_then(|fallback| fallback.find_config_file("alacritty.yml"))
        })
        .or_else(|| {
            if let Ok(home) = env::var("HOME") {
                // Fallback path: $HOME/.config/alacritty/alacritty.yml.
                let fallback = PathBuf::from(&home).join(".config/alacritty/alacritty.yml");
                if fallback.exists() {
                    return Some(fallback);
                }
                // Fallback path: $HOME/.alacritty.yml.
                let fallback = PathBuf::from(&home).join(".alacritty.yml");
                if fallback.exists() {
                    return Some(fallback);
                }
            }
            None
        })
}

#[cfg(windows)]
pub fn installed_config() -> Option<PathBuf> {
    dirs::config_dir().map(|path| path.join("alacritty\\alacritty.yml")).filter(|new| new.exists())
}

pub fn load_from(path: PathBuf) -> Config {
    let mut config = reload_from(&path).unwrap_or_else(|_| Config::default());
    config.config_path = Some(path);
    config
}

pub fn reload_from(path: &PathBuf) -> Result<Config> {
    match read_config(path) {
        Ok(config) => Ok(config),
        Err(err) => {
            error!(target: LOG_TARGET_CONFIG, "Unable to load config {:?}: {}", path, err);
            Err(err)
        },
    }
}

fn read_config(path: &PathBuf) -> Result<Config> {
    let mut contents = fs::read_to_string(path)?;

    // Remove UTF-8 BOM.
    if contents.starts_with('\u{FEFF}') {
        contents = contents.split_off(3);
    }

    parse_config(&contents)
}

fn parse_config(contents: &str) -> Result<Config> {
    match serde_yaml::from_str(contents) {
        Err(error) => {
            // Prevent parsing error with an empty string and commented out file.
            if error.to_string() == "EOF while parsing a value" {
                Ok(Config::default())
            } else {
                Err(Error::Yaml(error))
            }
        },
        Ok(config) => {
            print_deprecation_warnings(&config);
            Ok(config)
        },
    }
}

fn print_deprecation_warnings(config: &Config) {
    if config.scrolling.faux_multiplier().is_some() {
        warn!(
            target: LOG_TARGET_CONFIG,
            "Config scrolling.faux_multiplier is deprecated; the alternate scroll escape can now \
             be used to disable it and `scrolling.multiplier` controls the number of scrolled \
             lines"
        );
    }

    if config.scrolling.auto_scroll.is_some() {
        warn!(
            target: LOG_TARGET_CONFIG,
            "Config scrolling.auto_scroll has been removed and is now always disabled, it can be \
             safely removed from the config"
        );
    }

    if config.tabspaces.is_some() {
        warn!(
            target: LOG_TARGET_CONFIG,
            "Config tabspaces has been removed and is now always 8, it can be safely removed from \
             the config"
        );
    }

    if config.visual_bell.is_some() {
        warn!(
            target: LOG_TARGET_CONFIG,
            "Config visual_bell has been deprecated; please use bell instead"
        )
    }

    if config.ui_config.dynamic_title.is_some() {
        warn!(
            target: LOG_TARGET_CONFIG,
            "Config dynamic_title is deprecated; please use window.dynamic_title instead",
        )
    }
}

#[cfg(test)]
mod tests {
    static DEFAULT_ALACRITTY_CONFIG: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../alacritty.yml"));

    use super::Config;

    #[test]
    fn config_read_eof() {
        assert_eq!(super::parse_config(DEFAULT_ALACRITTY_CONFIG).unwrap(), Config::default());
    }
}
