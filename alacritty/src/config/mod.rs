use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;
use std::{env, fs, io};

use log::{error, info, warn};
use serde::Deserialize;
use serde_yaml::mapping::Mapping;
use serde_yaml::Value;

use alacritty_terminal::config::{Config as TermConfig, LOG_TARGET_CONFIG};

pub mod debug;
pub mod font;
pub mod monitor;
pub mod serde_utils;
pub mod ui_config;
pub mod window;

mod bindings;
mod mouse;

use crate::cli::Options;
pub use crate::config::bindings::{Action, Binding, Key, ViAction};
#[cfg(test)]
pub use crate::config::mouse::{ClickHandler, Mouse};
use crate::config::ui_config::UIConfig;

/// Maximum number of depth for the configuration file imports.
const IMPORT_RECURSION_LIMIT: usize = 5;

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

/// Load the configuration file.
pub fn load(options: &Options) -> Config {
    let config_options = options.config_options().clone();
    let config_path = options.config_path().or_else(installed_config);

    if config_path.is_none() {
        info!(target: LOG_TARGET_CONFIG, "No config file found; using default");
    }

    // Load the config using the following fallback behavior:
    //  - Config path + CLI overrides
    //  - CLI overrides
    //  - Default
    let mut config = config_path
        .and_then(|config_path| load_from(&config_path, config_options.clone()).ok())
        .unwrap_or_else(|| Config::deserialize(config_options).unwrap_or_default());

    // Override config with CLI options.
    options.override_config(&mut config);

    config
}

/// Attempt to reload the configuration file.
pub fn reload(config_path: &PathBuf, options: &Options) -> Result<Config> {
    // Load config, propagating errors.
    let config_options = options.config_options().clone();
    let mut config = load_from(&config_path, config_options)?;

    // Override config with CLI options.
    options.override_config(&mut config);

    Ok(config)
}

/// Load configuration file and log errors.
fn load_from(path: &PathBuf, cli_config: Value) -> Result<Config> {
    match read_config(path, cli_config) {
        Ok(config) => Ok(config),
        Err(err) => {
            error!(target: LOG_TARGET_CONFIG, "Unable to load config {:?}: {}", path, err);
            Err(err)
        },
    }
}

/// Deserialize configuration file from path.
fn read_config(path: &PathBuf, cli_config: Value) -> Result<Config> {
    let mut config_paths = Vec::new();
    let mut config_value = parse_config(&path, &mut config_paths, IMPORT_RECURSION_LIMIT)?;

    // Override config with CLI options.
    config_value = serde_utils::merge(config_value, cli_config);

    // Deserialize to concrete type.
    let mut config = Config::deserialize(config_value)?;
    config.ui_config.config_paths = config_paths;

    print_deprecation_warnings(&config);

    Ok(config)
}

/// Deserialize all configuration files as generic Value.
fn parse_config(
    path: &PathBuf,
    config_paths: &mut Vec<PathBuf>,
    recursion_limit: usize,
) -> Result<Value> {
    config_paths.push(path.to_owned());

    let mut contents = fs::read_to_string(path)?;

    // Remove UTF-8 BOM.
    if contents.starts_with('\u{FEFF}') {
        contents = contents.split_off(3);
    }

    // Load configuration file as Value.
    let config: Value = match serde_yaml::from_str(&contents) {
        Ok(config) => config,
        Err(error) => {
            // Prevent parsing error with an empty string and commented out file.
            if error.to_string() == "EOF while parsing a value" {
                Value::Mapping(Mapping::new())
            } else {
                return Err(Error::Yaml(error));
            }
        },
    };

    // Merge config with imports.
    let imports = load_imports(&config, config_paths, recursion_limit);
    Ok(serde_utils::merge(imports, config))
}

/// Load all referenced configuration files.
fn load_imports(config: &Value, config_paths: &mut Vec<PathBuf>, recursion_limit: usize) -> Value {
    let imports = match config.get("import") {
        Some(Value::Sequence(imports)) => imports,
        Some(_) => {
            error!(target: LOG_TARGET_CONFIG, "Invalid import type: expected a sequence");
            return Value::Null;
        },
        None => return Value::Null,
    };

    // Limit recursion to prevent infinite loops.
    if !imports.is_empty() && recursion_limit == 0 {
        error!(target: LOG_TARGET_CONFIG, "Exceeded maximum configuration import depth");
        return Value::Null;
    }

    let mut merged = Value::Null;

    for import in imports {
        let path = match import {
            Value::String(path) => PathBuf::from(path),
            _ => {
                error!(
                    target: LOG_TARGET_CONFIG,
                    "Invalid import element type: expected path string"
                );
                continue;
            },
        };

        match parse_config(&path, config_paths, recursion_limit - 1) {
            Ok(config) => merged = serde_utils::merge(merged, config),
            Err(err) => {
                error!(target: LOG_TARGET_CONFIG, "Unable to import config {:?}: {}", path, err)
            },
        }
    }

    merged
}

/// Get the location of the first found default config file paths
/// according to the following order:
///
/// 1. $XDG_CONFIG_HOME/alacritty/alacritty.yml
/// 2. $XDG_CONFIG_HOME/alacritty.yml
/// 3. $HOME/.config/alacritty/alacritty.yml
/// 4. $HOME/.alacritty.yml
#[cfg(not(windows))]
fn installed_config() -> Option<PathBuf> {
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
fn installed_config() -> Option<PathBuf> {
    dirs::config_dir().map(|path| path.join("alacritty\\alacritty.yml")).filter(|new| new.exists())
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
    use super::*;

    static DEFAULT_ALACRITTY_CONFIG: &str =
        concat!(env!("CARGO_MANIFEST_DIR"), "/../alacritty.yml");

    #[test]
    fn config_read_eof() {
        let config_path: PathBuf = DEFAULT_ALACRITTY_CONFIG.into();
        let mut config = read_config(&config_path, Value::Null).unwrap();
        config.ui_config.config_paths = Vec::new();
        assert_eq!(config, Config::default());
    }
}
