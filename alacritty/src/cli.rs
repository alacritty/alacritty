use std::cmp::max;
use std::path::PathBuf;

use clap::{crate_authors, crate_description, crate_name, crate_version, App, Arg};
use log::{self, error, LevelFilter};
use serde_yaml::Value;

use alacritty_terminal::config::Program;

use crate::config::serde_utils;
use crate::config::window::DEFAULT_NAME;
use crate::config::Config;

#[cfg(not(any(target_os = "macos", windows)))]
const CONFIG_PATH: &str = "$XDG_CONFIG_HOME/alacritty/alacritty.yml";
#[cfg(windows)]
const CONFIG_PATH: &str = "%APPDATA%\\alacritty\\alacritty.yml";
#[cfg(target_os = "macos")]
const CONFIG_PATH: &str = "$HOME/.config/alacritty/alacritty.yml";

/// Options specified on the command line.
pub struct Options {
    pub print_events: bool,
    pub ref_test: bool,
    pub title: Option<String>,
    pub class_instance: Option<String>,
    pub class_general: Option<String>,
    pub embed: Option<String>,
    pub log_level: LevelFilter,
    pub command: Option<Program>,
    pub hold: bool,
    pub working_directory: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub config_options: Value,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            print_events: false,
            ref_test: false,
            title: None,
            class_instance: None,
            class_general: None,
            embed: None,
            log_level: LevelFilter::Warn,
            command: None,
            hold: false,
            working_directory: None,
            config_path: None,
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
                Arg::with_name("print-events")
                    .long("print-events")
                    .help("Print all events to stdout"),
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

        if let Some(command) = &self.command {
            config.shell = Some(command.clone());
        }

        config.hold = self.hold;

        if let Some(title) = self.title.clone() {
            config.ui_config.window.title = title
        }
        if let Some(class_instance) = self.class_instance.clone() {
            config.ui_config.window.class.instance = class_instance;
        }
        if let Some(class_general) = self.class_general.clone() {
            config.ui_config.window.class.general = class_general;
        }

        config.ui_config.window.dynamic_title &= self.title.is_none();
        config.ui_config.window.embed = self.embed.as_ref().and_then(|embed| embed.parse().ok());
        config.ui_config.debug.print_events |= self.print_events;
        config.ui_config.debug.log_level = max(config.ui_config.debug.log_level, self.log_level);
        config.ui_config.debug.ref_test |= self.ref_test;

        if config.ui_config.debug.print_events {
            config.ui_config.debug.log_level =
                max(config.ui_config.debug.log_level, LevelFilter::Info);
        }
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
        let old_dynamic_title = config.ui_config.window.dynamic_title;

        Options::default().override_config(&mut config);

        assert_eq!(old_dynamic_title, config.ui_config.window.dynamic_title);
    }

    #[test]
    fn dynamic_title_overridden_by_options() {
        let mut config = Config::default();

        let options = Options { title: Some("foo".to_owned()), ..Options::default() };
        options.override_config(&mut config);

        assert!(!config.ui_config.window.dynamic_title);
    }

    #[test]
    fn dynamic_title_not_overridden_by_config() {
        let mut config = Config::default();

        config.ui_config.window.title = "foo".to_owned();
        Options::default().override_config(&mut config);

        assert!(config.ui_config.window.dynamic_title);
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
