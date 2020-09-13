use std::cmp::max;
use std::path::PathBuf;

use clap::{crate_authors, crate_description, crate_name, crate_version, App, Arg};
use log::{self, error, LevelFilter};
use serde_yaml::Value;

use alacritty_terminal::config::Program;
use alacritty_terminal::index::{Column, Line};

use crate::config::serde_utils;
use crate::config::ui_config::Delta;
use crate::config::window::{Dimensions, DEFAULT_NAME};
use crate::config::Config;

#[cfg(not(any(target_os = "macos", windows)))]
const CONFIG_PATH: &str = "$XDG_CONFIG_HOME/alacritty/alacritty.yml";
#[cfg(windows)]
const CONFIG_PATH: &str = "%APPDATA%\\alacritty\\alacritty.yml";
#[cfg(target_os = "macos")]
const CONFIG_PATH: &str = "$HOME/.config/alacritty/alacritty.yml";

/// Options specified on the command line.
pub struct Options {
    pub live_config_reload: Option<bool>,
    pub print_events: bool,
    pub ref_test: bool,
    pub dimensions: Option<Dimensions>,
    pub position: Option<Delta<i32>>,
    pub title: Option<String>,
    pub class_instance: Option<String>,
    pub class_general: Option<String>,
    pub embed: Option<String>,
    pub log_level: LevelFilter,
    pub command: Option<Program>,
    pub hold: bool,
    pub working_directory: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub persistent_logging: bool,
    pub config_options: Value,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            live_config_reload: None,
            print_events: false,
            ref_test: false,
            dimensions: None,
            position: None,
            title: None,
            class_instance: None,
            class_general: None,
            embed: None,
            log_level: LevelFilter::Warn,
            command: None,
            hold: false,
            working_directory: None,
            config_path: None,
            persistent_logging: false,
            config_options: Value::Null,
        }
    }
}

impl Options {
    /// Build `Options` from command line arguments.
    pub fn new() -> Self {
        let mut version = crate_version!().to_owned();
        let commit_hash = env!("GIT_HASH");
        if !commit_hash.is_empty() {
            version = format!("{} ({})", version, commit_hash);
        }

        let mut options = Options::default();

        let matches = App::new(crate_name!())
            .version(version.as_str())
            .author(crate_authors!("\n"))
            .about(crate_description!())
            .arg(Arg::with_name("ref-test").long("ref-test").help("Generates ref test"))
            .arg(
                Arg::with_name("live-config-reload")
                    .long("live-config-reload")
                    .help("Enable automatic config reloading"),
            )
            .arg(
                Arg::with_name("no-live-config-reload")
                    .long("no-live-config-reload")
                    .help("Disable automatic config reloading")
                    .conflicts_with("live-config-reload"),
            )
            .arg(
                Arg::with_name("print-events")
                    .long("print-events")
                    .help("Print all events to stdout"),
            )
            .arg(
                Arg::with_name("persistent-logging")
                    .long("persistent-logging")
                    .help("Keep the log file after quitting Alacritty"),
            )
            .arg(
                Arg::with_name("dimensions")
                    .long("dimensions")
                    .short("d")
                    .value_names(&["columns", "lines"])
                    .help(
                        "Defines the window dimensions. Falls back to size specified by window \
                         manager if set to 0x0 [default: 0x0]",
                    ),
            )
            .arg(
                Arg::with_name("position")
                    .long("position")
                    .allow_hyphen_values(true)
                    .value_names(&["x-pos", "y-pos"])
                    .help(
                        "Defines the window position. Falls back to position specified by window \
                         manager if unset [default: unset]",
                    ),
            )
            .arg(
                Arg::with_name("title")
                    .long("title")
                    .short("t")
                    .takes_value(true)
                    .help(&format!("Defines the window title [default: {}]", DEFAULT_NAME)),
            )
            .arg(
                Arg::with_name("class")
                    .long("class")
                    .value_name("instance> | <instance>,<general")
                    .takes_value(true)
                    .use_delimiter(true)
                    .help(&format!(
                        "Defines window class/app_id on X11/Wayland [default: {}]",
                        DEFAULT_NAME
                    )),
            )
            .arg(
                Arg::with_name("embed").long("embed").takes_value(true).help(
                    "Defines the X11 window ID (as a decimal integer) to embed Alacritty within",
                ),
            )
            .arg(
                Arg::with_name("q")
                    .short("q")
                    .multiple(true)
                    .conflicts_with("v")
                    .help("Reduces the level of verbosity (the min level is -qq)"),
            )
            .arg(
                Arg::with_name("v")
                    .short("v")
                    .multiple(true)
                    .conflicts_with("q")
                    .help("Increases the level of verbosity (the max level is -vvv)"),
            )
            .arg(
                Arg::with_name("working-directory")
                    .long("working-directory")
                    .takes_value(true)
                    .help("Start the shell in the specified working directory"),
            )
            .arg(Arg::with_name("config-file").long("config-file").takes_value(true).help(
                &format!("Specify alternative configuration file [default: {}]", CONFIG_PATH),
            ))
            .arg(
                Arg::with_name("command")
                    .long("command")
                    .short("e")
                    .multiple(true)
                    .takes_value(true)
                    .allow_hyphen_values(true)
                    .help("Command and args to execute (must be last argument)"),
            )
            .arg(Arg::with_name("hold").long("hold").help("Remain open after child process exits"))
            .arg(
                Arg::with_name("option")
                    .long("option")
                    .short("o")
                    .multiple(true)
                    .takes_value(true)
                    .help("Override configuration file options [example: cursor.style=Beam]"),
            )
            .get_matches();

        if matches.is_present("ref-test") {
            options.ref_test = true;
        }

        if matches.is_present("print-events") {
            options.print_events = true;
        }

        if matches.is_present("live-config-reload") {
            options.live_config_reload = Some(true);
        } else if matches.is_present("no-live-config-reload") {
            options.live_config_reload = Some(false);
        }

        if matches.is_present("persistent-logging") {
            options.persistent_logging = true;
        }

        if let Some(mut dimensions) = matches.values_of("dimensions") {
            let width = dimensions.next().map(|w| w.parse().map(Column));
            let height = dimensions.next().map(|h| h.parse().map(Line));
            if let (Some(Ok(width)), Some(Ok(height))) = (width, height) {
                options.dimensions = Some(Dimensions::new(width, height));
            }
        }

        if let Some(mut position) = matches.values_of("position") {
            let x = position.next().map(str::parse);
            let y = position.next().map(str::parse);
            if let (Some(Ok(x)), Some(Ok(y))) = (x, y) {
                options.position = Some(Delta { x, y });
            }
        }

        if let Some(mut class) = matches.values_of("class") {
            options.class_instance = class.next().map(|instance| instance.to_owned());
            options.class_general = class.next().map(|general| general.to_owned());
        }

        options.title = matches.value_of("title").map(ToOwned::to_owned);
        options.embed = matches.value_of("embed").map(ToOwned::to_owned);

        match matches.occurrences_of("q") {
            0 => (),
            1 => options.log_level = LevelFilter::Error,
            _ => options.log_level = LevelFilter::Off,
        }

        match matches.occurrences_of("v") {
            0 if !options.print_events => options.log_level = LevelFilter::Warn,
            0 | 1 => options.log_level = LevelFilter::Info,
            2 => options.log_level = LevelFilter::Debug,
            _ => options.log_level = LevelFilter::Trace,
        }

        if let Some(dir) = matches.value_of("working-directory") {
            options.working_directory = Some(PathBuf::from(dir.to_string()));
        }

        if let Some(path) = matches.value_of("config-file") {
            options.config_path = Some(PathBuf::from(path.to_string()));
        }

        if let Some(mut args) = matches.values_of("command") {
            // The following unwrap is guaranteed to succeed.
            // If `command` exists it must also have a first item since
            // `Arg::min_values(1)` is set.
            let program = String::from(args.next().unwrap());
            let args = args.map(String::from).collect();
            options.command = Some(Program::WithArgs { program, args });
        }

        if matches.is_present("hold") {
            options.hold = true;
        }

        if let Some(config_options) = matches.values_of("option") {
            for option in config_options {
                match option_as_value(option) {
                    Ok(value) => {
                        options.config_options = serde_utils::merge(options.config_options, value);
                    },
                    Err(_) => eprintln!("Invalid CLI config option: {:?}", option),
                }
            }
        }

        options
    }

    /// Configuration file path.
    pub fn config_path(&self) -> Option<PathBuf> {
        self.config_path.clone()
    }

    /// CLI config options as deserializable serde value.
    pub fn config_options(&self) -> &Value {
        &self.config_options
    }

    /// Override configuration file with options from the CLI.
    pub fn override_config(&self, config: &mut Config) {
        if let Some(working_directory) = &self.working_directory {
            if working_directory.is_dir() {
                config.working_directory = Some(working_directory.to_owned());
            } else {
                error!("Invalid working directory: {:?}", working_directory);
            }
        }

        if let Some(lcr) = self.live_config_reload {
            config.ui_config.set_live_config_reload(lcr);
        }

        if let Some(command) = &self.command {
            config.shell = Some(command.clone());
        }

        config.hold = self.hold;

        let dynamic_title = config.ui_config.dynamic_title() && self.title.is_none();
        config.ui_config.set_dynamic_title(dynamic_title);

        replace_if_some(&mut config.ui_config.window.dimensions, self.dimensions);
        replace_if_some(&mut config.ui_config.window.title, self.title.clone());
        replace_if_some(&mut config.ui_config.window.class.instance, self.class_instance.clone());
        replace_if_some(&mut config.ui_config.window.class.general, self.class_general.clone());

        config.ui_config.window.position = self.position.or(config.ui_config.window.position);
        config.ui_config.window.embed = self.embed.as_ref().and_then(|embed| embed.parse().ok());
        config.ui_config.debug.print_events |= self.print_events;
        config.ui_config.debug.log_level = max(config.ui_config.debug.log_level, self.log_level);
        config.ui_config.debug.ref_test |= self.ref_test;
        config.ui_config.debug.persistent_logging |= self.persistent_logging;

        if config.ui_config.debug.print_events {
            config.ui_config.debug.log_level =
                max(config.ui_config.debug.log_level, LevelFilter::Info);
        }
    }
}

fn replace_if_some<T>(option: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *option = value;
    }
}

/// Format an option in the format of `parent.field=value` to a serde Value.
fn option_as_value(option: &str) -> Result<Value, serde_yaml::Error> {
    let mut yaml_text = String::with_capacity(option.len());
    let mut closing_brackets = String::new();

    for (i, c) in option.chars().enumerate() {
        match c {
            '=' => {
                yaml_text.push_str(": ");
                yaml_text.push_str(&option[i + 1..]);
                break;
            },
            '.' => {
                yaml_text.push_str(": {");
                closing_brackets.push('}');
            },
            _ => yaml_text.push(c),
        }
    }

    yaml_text += &closing_brackets;

    serde_yaml::from_str(&yaml_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_yaml::mapping::Mapping;

    #[test]
    fn dynamic_title_ignoring_options_by_default() {
        let mut config = Config::default();
        let old_dynamic_title = config.ui_config.dynamic_title();

        Options::default().override_config(&mut config);

        assert_eq!(old_dynamic_title, config.ui_config.dynamic_title());
    }

    #[test]
    fn dynamic_title_overridden_by_options() {
        let mut config = Config::default();

        let mut options = Options::default();
        options.title = Some("foo".to_owned());
        options.override_config(&mut config);

        assert!(!config.ui_config.dynamic_title());
    }

    #[test]
    fn dynamic_title_not_overridden_by_config() {
        let mut config = Config::default();

        config.ui_config.window.title = "foo".to_owned();
        Options::default().override_config(&mut config);

        assert!(config.ui_config.dynamic_title());
    }

    #[test]
    fn valid_option_as_value() {
        // Test with a single field.
        let value = option_as_value("field=true").unwrap();

        let mut mapping = Mapping::new();
        mapping.insert(Value::String(String::from("field")), Value::Bool(true));

        assert_eq!(value, Value::Mapping(mapping));

        // Test with nested fields
        let value = option_as_value("parent.field=true").unwrap();

        let mut parent_mapping = Mapping::new();
        parent_mapping.insert(Value::String(String::from("field")), Value::Bool(true));
        let mut mapping = Mapping::new();
        mapping.insert(Value::String(String::from("parent")), Value::Mapping(parent_mapping));

        assert_eq!(value, Value::Mapping(mapping));
    }

    #[test]
    fn invalid_option_as_value() {
        let value = option_as_value("}");
        assert!(value.is_err());
    }

    #[test]
    fn float_option_as_value() {
        let value = option_as_value("float=3.4").unwrap();

        let mut expected = Mapping::new();
        expected.insert(Value::String(String::from("float")), Value::Number(3.4.into()));

        assert_eq!(value, Value::Mapping(expected));
    }
}
