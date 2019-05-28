use std::borrow::Cow;
use std::env;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

#[cfg(windows)]
use dirs;
use log::{error, warn};
use serde_yaml;
#[cfg(not(windows))]
use xdg;

use alacritty_terminal::config::{Config, DEFAULT_ALACRITTY_CONFIG};

pub const SOURCE_FILE_PATH: &str = file!();

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

impl ::std::error::Error for Error {
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

impl ::std::fmt::Display for Error {
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
pub fn installed_config<'a>() -> Option<Cow<'a, Path>> {
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
        .map(Into::into)
}

#[cfg(windows)]
pub fn installed_config<'a>() -> Option<Cow<'a, Path>> {
    dirs::config_dir()
        .map(|path| path.join("alacritty\\alacritty.yml"))
        .filter(|new| new.exists())
        .map(Cow::from)
}

#[cfg(not(windows))]
pub fn write_defaults() -> io::Result<Cow<'static, Path>> {
    let path = xdg::BaseDirectories::with_prefix("alacritty")
        .map_err(|err| io::Error::new(io::ErrorKind::NotFound, err.to_string().as_str()))
        .and_then(|p| p.place_config_file("alacritty.yml"))?;

    File::create(&path)?.write_all(DEFAULT_ALACRITTY_CONFIG.as_bytes())?;

    Ok(path.into())
}

#[cfg(windows)]
pub fn write_defaults() -> io::Result<Cow<'static, Path>> {
    let mut path = dirs::config_dir().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Couldn't find profile directory")
    })?;

    path = path.join("alacritty/alacritty.yml");

    std::fs::create_dir_all(path.parent().unwrap())?;

    File::create(&path)?.write_all(DEFAULT_ALACRITTY_CONFIG.as_bytes())?;

    Ok(path.into())
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
            error!("Unable to load config {:?}: {}", path, err);
            Err(err)
        },
    }
}

fn read_config(path: &PathBuf) -> Result<Config> {
    let mut contents = String::new();
    File::open(path)?.read_to_string(&mut contents)?;

    // Remove UTF-8 BOM
    if contents.chars().nth(0) == Some('\u{FEFF}') {
        contents = contents.split_off(3);
    }

    // Prevent parsing error with empty string
    if contents.is_empty() {
        return Ok(Config::default());
    }

    let config = serde_yaml::from_str(&contents)?;

    print_deprecation_warnings(&config);

    Ok(config)
}

fn print_deprecation_warnings(config: &Config) {
    if config.window.start_maximized.is_some() {
        warn!(
            "Config window.start_maximized is deprecated; please use window.startup_mode instead"
        );
    }

    if config.render_timer.is_some() {
        warn!("Config render_timer is deprecated; please use debug.render_timer instead");
    }

    if config.persistent_logging.is_some() {
        warn!(
            "Config persistent_logging is deprecated; please use debug.persistent_logging instead"
        );
    }
}
