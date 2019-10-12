use std::env;
use std::io;
use std::path::PathBuf;

#[cfg(windows)]
use dirs;
use log::{error, warn};
use serde_yaml;
#[cfg(not(windows))]
use xdg;

use alacritty_terminal::config::{Config as TermConfig, LOG_TARGET_CONFIG};

mod bindings;
pub mod monitor;
mod mouse;
mod ui_config;

pub use crate::config::bindings::{Action, Binding, Key, RelaxedEq};
#[cfg(test)]
pub use crate::config::mouse::{ClickHandler, Mouse};
use crate::config::ui_config::UIConfig;

pub type Config = TermConfig<UIConfig>;

/// Result from config loading
pub type Result<T> = ::std::result::Result<T, Error>;

/// Errors occurring during config loading
#[derive(Debug)]
pub enum Error {
    /// Config file not found
    NotFound,

    /// Couldn't read $HOME environment variable
    ReadingEnvHome(env::VarError),

    /// io error reading file
    Io(io::Error),

    /// Not valid yaml or missing parameters
    Yaml(serde_yaml::Error),
}

impl std::error::Error for Error {
    fn cause(&self) -> Option<&dyn (::std::error::Error)> {
        match *self {
            Error::NotFound => None,
            Error::ReadingEnvHome(ref err) => Some(err),
            Error::Io(ref err) => Some(err),
            Error::Yaml(ref err) => Some(err),
        }
    }

    fn description(&self) -> &str {
        match *self {
            Error::NotFound => "Couldn't locate config file",
            Error::ReadingEnvHome(ref err) => err.description(),
            Error::Io(ref err) => err.description(),
            Error::Yaml(ref err) => err.description(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match *self {
            Error::NotFound => write!(f, "{}", ::std::error::Error::description(self)),
            Error::ReadingEnvHome(ref err) => {
                write!(f, "Couldn't read $HOME environment variable: {}", err)
            },
            Error::Io(ref err) => write!(f, "Error reading config file: {}", err),
            Error::Yaml(ref err) => write!(f, "Problem with config: {}", err),
        }
    }
}

impl From<env::VarError> for Error {
    fn from(val: env::VarError) -> Error {
        Error::ReadingEnvHome(val)
    }
}

impl From<io::Error> for Error {
    fn from(val: io::Error) -> Error {
        if val.kind() == io::ErrorKind::NotFound {
            Error::NotFound
        } else {
            Error::Io(val)
        }
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(val: serde_yaml::Error) -> Error {
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
    // Try using XDG location by default
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
                // Fallback path: $HOME/.config/alacritty/alacritty.yml
                let fallback = PathBuf::from(&home).join(".config/alacritty/alacritty.yml");
                if fallback.exists() {
                    return Some(fallback);
                }
                // Fallback path: $HOME/.alacritty.yml
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
    let mut contents = std::fs::read_to_string(path)?;

    // Remove UTF-8 BOM
    if contents.chars().nth(0) == Some('\u{FEFF}') {
        contents = contents.split_off(3);
    }

    parse_config(&contents)
}

fn parse_config(contents: &str) -> Result<Config> {
    match serde_yaml::from_str(&contents) {
        Err(error) => {
            // Prevent parsing error with an empty string and commented out file.
            if std::error::Error::description(&error) == "EOF while parsing a value" {
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
    if config.window.start_maximized.is_some() {
        warn!(
            target: LOG_TARGET_CONFIG,
            "Config window.start_maximized is deprecated; please use window.startup_mode instead"
        );
    }

    if config.render_timer.is_some() {
        warn!(
            target: LOG_TARGET_CONFIG,
            "Config render_timer is deprecated; please use debug.render_timer instead"
        );
    }

    if config.persistent_logging.is_some() {
        warn!(
            target: LOG_TARGET_CONFIG,
            "Config persistent_logging is deprecated; please use debug.persistent_logging instead"
        );
    }

    if config.scrolling.faux_multiplier().is_some() {
        warn!(
            target: LOG_TARGET_CONFIG,
            "Config scrolling.faux_multiplier is deprecated; please use scrolling.multiplier for \
             both usual and faux scrolling. You can enable/disable faux scrolling using \
             `\\e[?1007h`/`\\e[?1007l` escape sequences respectively"
        );
    }
}

#[cfg(test)]
mod test {
    static DEFAULT_ALACRITTY_CONFIG: &str =
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../alacritty.yml"));

    use super::Config;

    #[test]
    fn config_read_eof() {
        assert_eq!(super::parse_config(DEFAULT_ALACRITTY_CONFIG).unwrap(), Config::default());
    }
}
