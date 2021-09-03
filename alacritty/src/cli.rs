use std::cmp::max;
use std::path::PathBuf;

use log::{self, error, LevelFilter};
use serde_yaml::Value;
use structopt::StructOpt;

use alacritty_terminal::config::Program;

use crate::config::window::{Class, DEFAULT_NAME};
use crate::config::{serde_utils, Config};

/// CLI options for the main Alacritty executable.
#[derive(StructOpt, Debug)]
#[structopt(author, about, version = env!("VERSION"))]
pub struct Options {
    /// Print all events to stdout.
    #[structopt(long)]
    pub print_events: bool,

    /// Generates ref test.
    #[structopt(long)]
    pub ref_test: bool,

    /// Defines the window title [default: Alacritty].
    #[structopt(short, long)]
    pub title: Option<String>,

    /// Defines window class/app_id on X11/Wayland [default: Alacritty].
    #[structopt(long, value_name = "instance> | <instance>,<general", parse(try_from_str = parse_class))]
    pub class: Option<Class>,

    /// Defines the X11 window ID (as a decimal integer) to embed Alacritty within.
    #[structopt(long)]
    pub embed: Option<String>,

    /// Start the shell in the specified working directory.
    #[structopt(long)]
    pub working_directory: Option<PathBuf>,

    /// Specify alternative configuration file [default: $XDG_CONFIG_HOME/alacritty/alacritty.yml].
    #[cfg(not(any(target_os = "macos", windows)))]
    #[structopt(long)]
    pub config_file: Option<PathBuf>,

    /// Specify alternative configuration file [default: %APPDATA%\alacritty\alacritty.yml].
    #[cfg(windows)]
    #[structopt(long)]
    pub config_file: Option<PathBuf>,

    /// Specify alternative configuration file [default: $HOME/.config/alacritty/alacritty.yml].
    #[cfg(target_os = "macos")]
    #[structopt(long)]
    pub config_file: Option<PathBuf>,

    /// Remain open after child process exits.
    #[structopt(long)]
    pub hold: bool,

    /// Path for IPC socket creation.
    #[cfg(unix)]
    #[structopt(long)]
    pub socket: Option<PathBuf>,

    /// Reduces the level of verbosity (the min level is -qq).
    #[structopt(short, conflicts_with("verbose"), parse(from_occurrences))]
    quiet: u8,

    /// Increases the level of verbosity (the max level is -vvv).
    #[structopt(short, conflicts_with("quiet"), parse(from_occurrences))]
    verbose: u8,

    /// Command and args to execute (must be last argument).
    #[structopt(short = "e", long, allow_hyphen_values = true)]
    command: Vec<String>,

    /// Override configuration file options [example: cursor.style=Beam].
    #[structopt(short = "o", long)]
    option: Vec<String>,

    /// CLI options for config overrides.
    #[structopt(skip)]
    pub config_options: Value,

    /// Subcommand passed to the CLI.
    #[cfg(unix)]
    #[structopt(subcommand)]
    pub subcommands: Option<Subcommands>,
}

impl Options {
    pub fn new() -> Self {
        let mut options = Self::from_args();

        // Convert `--option` flags into serde `Value`.
        for option in &options.option {
            match option_as_value(option) {
                Ok(value) => {
                    options.config_options = serde_utils::merge(options.config_options, value);
                },
                Err(_) => eprintln!("Invalid CLI config option: {:?}", option),
            }
        }

        options
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

        if let Some(command) = self.command() {
            config.shell = Some(command);
        }

        config.hold = self.hold;

        if let Some(title) = self.title.clone() {
            config.ui_config.window.title = title
        }
        if let Some(class) = &self.class {
            config.ui_config.window.class = class.clone();
        }

        #[cfg(unix)]
        {
            config.ui_config.ipc_socket |= self.socket.is_some();
        }

        config.ui_config.window.dynamic_title &= self.title.is_none();
        config.ui_config.window.embed = self.embed.as_ref().and_then(|embed| embed.parse().ok());
        config.ui_config.debug.print_events |= self.print_events;
        config.ui_config.debug.log_level = max(config.ui_config.debug.log_level, self.log_level());
        config.ui_config.debug.ref_test |= self.ref_test;

        if config.ui_config.debug.print_events {
            config.ui_config.debug.log_level =
                max(config.ui_config.debug.log_level, LevelFilter::Info);
        }
    }

    /// Logging filter level.
    pub fn log_level(&self) -> LevelFilter {
        match (self.quiet, self.verbose) {
            // Force at least `Info` level for `--print-events`.
            (_, 0) if self.print_events => LevelFilter::Info,

            // Default.
            (0, 0) => LevelFilter::Warn,

            // Verbose.
            (_, 1) => LevelFilter::Info,
            (_, 2) => LevelFilter::Debug,
            (0, _) => LevelFilter::Trace,

            // Quiet.
            (1, _) => LevelFilter::Error,
            (..) => LevelFilter::Off,
        }
    }

    /// Shell override passed through the CLI.
    pub fn command(&self) -> Option<Program> {
        let (program, args) = self.command.split_first()?;
        Some(Program::WithArgs { program: program.clone(), args: args.to_vec() })
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

/// Parse the class CLI parameter.
fn parse_class(input: &str) -> Result<Class, String> {
    match input.find(',') {
        Some(position) => {
            let general = input[position + 1..].to_owned();

            // Warn the user if they've passed too many values.
            if general.contains(',') {
                return Err(String::from("Too many parameters"));
            }

            Ok(Class { instance: input[..position].into(), general })
        },
        None => Ok(Class { instance: input.into(), general: DEFAULT_NAME.into() }),
    }
}

/// Available CLI subcommands.
#[cfg(unix)]
#[derive(StructOpt, Debug)]
pub enum Subcommands {
    Msg(MessageOptions),
}

/// Send a message to the Alacritty socket.
#[cfg(unix)]
#[derive(StructOpt, Debug)]
pub struct MessageOptions {
    /// IPC socket connection path override.
    #[structopt(long, short)]
    pub socket: Option<PathBuf>,

    /// Message which should be sent.
    #[structopt(subcommand)]
    pub message: SocketMessage,
}

/// Available socket messages.
#[cfg(unix)]
#[derive(StructOpt, Debug)]
pub enum SocketMessage {
    /// Create a new window in the same Alacritty process.
    CreateWindow,
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_yaml::mapping::Mapping;

    #[test]
    fn dynamic_title_ignoring_options_by_default() {
        let mut config = Config::default();
        let old_dynamic_title = config.ui_config.window.dynamic_title;

        Options::new().override_config(&mut config);

        assert_eq!(old_dynamic_title, config.ui_config.window.dynamic_title);
    }

    #[test]
    fn dynamic_title_overridden_by_options() {
        let mut config = Config::default();

        let options = Options { title: Some("foo".to_owned()), ..Options::new() };
        options.override_config(&mut config);

        assert!(!config.ui_config.window.dynamic_title);
    }

    #[test]
    fn dynamic_title_not_overridden_by_config() {
        let mut config = Config::default();

        config.ui_config.window.title = "foo".to_owned();
        Options::new().override_config(&mut config);

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

    #[test]
    fn parse_instance_class() {
        let class = parse_class("one").unwrap();
        assert_eq!(class.instance, "one");
        assert_eq!(class.general, DEFAULT_NAME);
    }

    #[test]
    fn parse_general_class() {
        let class = parse_class("one,two").unwrap();
        assert_eq!(class.instance, "one");
        assert_eq!(class.general, "two");
    }

    #[test]
    fn parse_invalid_class() {
        let class = parse_class("one,two,three");
        assert!(class.is_err());
    }
}
