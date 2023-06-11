use std::cmp::max;
use std::os::raw::c_ulong;
use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand, ValueHint};
use log::{self, error, LevelFilter};
use serde::{Deserialize, Serialize};
use toml::{Table, Value};

use alacritty_terminal::config::{Program, PtyConfig};

use crate::config::window::{Class, Identity};
use crate::config::{serde_utils, UiConfig};

/// CLI options for the main Alacritty executable.
#[derive(Parser, Default, Debug)]
#[clap(author, about, version = env!("VERSION"))]
pub struct Options {
    /// Print all events to stdout.
    #[clap(long)]
    pub print_events: bool,

    /// Generates ref test.
    #[clap(long)]
    pub ref_test: bool,

    /// X11 window ID to embed Alacritty within (decimal or hexadecimal with "0x" prefix).
    #[clap(long)]
    pub embed: Option<String>,

    /// Specify alternative configuration file [default:
    /// $XDG_CONFIG_HOME/alacritty/alacritty.toml].
    #[cfg(not(any(target_os = "macos", windows)))]
    #[clap(long, value_hint = ValueHint::FilePath)]
    pub config_file: Option<PathBuf>,

    /// Specify alternative configuration file [default: %APPDATA%\alacritty\alacritty.toml].
    #[cfg(windows)]
    #[clap(long, value_hint = ValueHint::FilePath)]
    pub config_file: Option<PathBuf>,

    /// Specify alternative configuration file [default: $HOME/.config/alacritty/alacritty.toml].
    #[cfg(target_os = "macos")]
    #[clap(long, value_hint = ValueHint::FilePath)]
    pub config_file: Option<PathBuf>,

    /// Path for IPC socket creation.
    #[cfg(unix)]
    #[clap(long, value_hint = ValueHint::FilePath)]
    pub socket: Option<PathBuf>,

    /// Reduces the level of verbosity (the min level is -qq).
    #[clap(short, conflicts_with("verbose"), action = ArgAction::Count)]
    quiet: u8,

    /// Increases the level of verbosity (the max level is -vvv).
    #[clap(short, conflicts_with("quiet"), action = ArgAction::Count)]
    verbose: u8,

    /// Override configuration file options [example: cursor.style=Beam].
    #[clap(short = 'o', long, num_args = 1..)]
    option: Vec<String>,

    /// CLI options for config overrides.
    #[clap(skip)]
    pub config_options: TomlValue,

    /// Options which can be passed via IPC.
    #[clap(flatten)]
    pub window_options: WindowOptions,

    /// Subcommand passed to the CLI.
    #[clap(subcommand)]
    pub subcommands: Option<Subcommands>,
}

impl Options {
    pub fn new() -> Self {
        let mut options = Self::parse();

        // Convert `--option` flags into serde `Value`.
        options.config_options = TomlValue(options_as_value(&options.option));

        options
    }

    /// Override configuration file with options from the CLI.
    pub fn override_config(&self, config: &mut UiConfig) {
        #[cfg(unix)]
        {
            config.ipc_socket |= self.socket.is_some();
        }

        config.window.dynamic_title &= self.window_options.window_identity.title.is_none();
        config.window.embed = self.embed.as_ref().and_then(|embed| parse_hex_or_decimal(embed));
        config.debug.print_events |= self.print_events;
        config.debug.log_level = max(config.debug.log_level, self.log_level());
        config.debug.ref_test |= self.ref_test;

        if config.debug.print_events {
            config.debug.log_level = max(config.debug.log_level, LevelFilter::Info);
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
}

/// Combine multiple options into a [`toml::Value`].
pub fn options_as_value(options: &[String]) -> Value {
    options.iter().fold(Value::Table(Table::new()), |value, option| match toml::from_str(option) {
        Ok(new_value) => serde_utils::merge(value, new_value),
        Err(_) => {
            eprintln!("Ignoring invalid option: {:?}", option);
            value
        },
    })
}

/// Parse the class CLI parameter.
fn parse_class(input: &str) -> Result<Class, String> {
    let (general, instance) = match input.split_once(',') {
        // Warn the user if they've passed too many values.
        Some((_, instance)) if instance.contains(',') => {
            return Err(String::from("Too many parameters"))
        },
        Some((general, instance)) => (general, instance),
        None => (input, input),
    };

    Ok(Class::new(general, instance))
}

/// Convert to hex if possible, else decimal
fn parse_hex_or_decimal(input: &str) -> Option<c_ulong> {
    input
        .strip_prefix("0x")
        .and_then(|value| c_ulong::from_str_radix(value, 16).ok())
        .or_else(|| input.parse().ok())
}

/// Terminal specific cli options which can be passed to new windows via IPC.
#[derive(Serialize, Deserialize, Args, Default, Debug, Clone, PartialEq, Eq)]
pub struct TerminalOptions {
    /// Start the shell in the specified working directory.
    #[clap(long, value_hint = ValueHint::FilePath)]
    pub working_directory: Option<PathBuf>,

    /// Remain open after child process exit.
    #[clap(long)]
    pub hold: bool,

    /// Command and args to execute (must be last argument).
    #[clap(short = 'e', long, allow_hyphen_values = true, num_args = 1..)]
    command: Vec<String>,
}

impl TerminalOptions {
    /// Shell override passed through the CLI.
    pub fn command(&self) -> Option<Program> {
        let (program, args) = self.command.split_first()?;
        Some(Program::WithArgs { program: program.clone(), args: args.to_vec() })
    }

    /// Override the [`PtyConfig`]'s fields with the [`TerminalOptions`].
    pub fn override_pty_config(&self, pty_config: &mut PtyConfig) {
        if let Some(working_directory) = &self.working_directory {
            if working_directory.is_dir() {
                pty_config.working_directory = Some(working_directory.to_owned());
            } else {
                error!("Invalid working directory: {:?}", working_directory);
            }
        }

        if let Some(command) = self.command() {
            pty_config.shell = Some(command);
        }

        pty_config.hold |= self.hold;
    }
}

impl From<TerminalOptions> for PtyConfig {
    fn from(mut options: TerminalOptions) -> Self {
        PtyConfig {
            working_directory: options.working_directory.take(),
            shell: options.command(),
            hold: options.hold,
        }
    }
}

/// Window specific cli options which can be passed to new windows via IPC.
#[derive(Serialize, Deserialize, Args, Default, Debug, Clone, PartialEq, Eq)]
pub struct WindowIdentity {
    /// Defines the window title [default: Alacritty].
    #[clap(short = 'T', short_alias('t'), long)]
    pub title: Option<String>,

    /// Defines window class/app_id on X11/Wayland [default: Alacritty].
    #[clap(long, value_name = "general> | <general>,<instance", value_parser = parse_class)]
    pub class: Option<Class>,
}

impl WindowIdentity {
    /// Override the [`WindowIdentity`]'s fields with the [`WindowOptions`].
    pub fn override_identity_config(&self, identity: &mut Identity) {
        if let Some(title) = &self.title {
            identity.title = title.clone();
        }
        if let Some(class) = &self.class {
            identity.class = class.clone();
        }
    }
}

/// Available CLI subcommands.
#[derive(Subcommand, Debug)]
pub enum Subcommands {
    #[cfg(unix)]
    Msg(MessageOptions),
    Migrate(MigrateOptions),
}

/// Send a message to the Alacritty socket.
#[cfg(unix)]
#[derive(Args, Debug)]
pub struct MessageOptions {
    /// IPC socket connection path override.
    #[clap(short, long, value_hint = ValueHint::FilePath)]
    pub socket: Option<PathBuf>,

    /// Message which should be sent.
    #[clap(subcommand)]
    pub message: SocketMessage,
}

/// Available socket messages.
#[cfg(unix)]
#[derive(Subcommand, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum SocketMessage {
    /// Create a new window in the same Alacritty process.
    CreateWindow(WindowOptions),

    /// Update the Alacritty configuration.
    Config(IpcConfig),
}

/// Migrate the configuration file.
#[derive(Args, Clone, Debug)]
pub struct MigrateOptions {
    /// Path to the configuration file.
    #[clap(short, long, value_hint = ValueHint::FilePath)]
    pub config_file: Option<PathBuf>,

    /// Only output TOML config to stdout.
    #[clap(short, long)]
    pub dry_run: bool,

    /// Do not recurse over imports.
    #[clap(short = 'i', long)]
    pub skip_imports: bool,

    /// Do not move renamed fields to their new location.
    #[clap(long)]
    pub skip_renames: bool,

    #[clap(short, long)]
    /// Do not output to STDOUT.
    pub silent: bool,
}

/// Subset of options that we pass to 'create-window' IPC subcommand.
#[derive(Serialize, Deserialize, Args, Default, Clone, Debug, PartialEq, Eq)]
pub struct WindowOptions {
    /// Terminal options which can be passed via IPC.
    #[clap(flatten)]
    pub terminal_options: TerminalOptions,

    #[clap(flatten)]
    /// Window options which could be passed via IPC.
    pub window_identity: WindowIdentity,
}

/// Parameters to the `config` IPC subcommand.
#[cfg(unix)]
#[derive(Args, Serialize, Deserialize, Default, Debug, Clone, PartialEq, Eq)]
pub struct IpcConfig {
    /// Configuration file options [example: cursor.style=Beam].
    #[clap(required = true, value_name = "CONFIG_OPTIONS")]
    pub options: Vec<String>,

    /// Window ID for the new config.
    ///
    /// Use `-1` to apply this change to all windows.
    #[clap(short, long, allow_hyphen_values = true, env = "ALACRITTY_WINDOW_ID")]
    pub window_id: Option<i128>,

    /// Clear all runtime configuration changes.
    #[clap(short, long, conflicts_with = "options")]
    pub reset: bool,
}

/// Toml value with default implementation.
#[derive(Debug)]
pub struct TomlValue(pub Value);

impl Default for TomlValue {
    fn default() -> Self {
        Self(Value::Table(Table::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    use std::fs::File;
    #[cfg(target_os = "linux")]
    use std::io::Read;

    #[cfg(target_os = "linux")]
    use clap::CommandFactory;
    #[cfg(target_os = "linux")]
    use clap_complete::Shell;
    use toml::Table;

    #[test]
    fn dynamic_title_ignoring_options_by_default() {
        let mut config = UiConfig::default();
        let old_dynamic_title = config.window.dynamic_title;

        Options::default().override_config(&mut config);

        assert_eq!(old_dynamic_title, config.window.dynamic_title);
    }

    #[test]
    fn dynamic_title_overridden_by_options() {
        let mut config = UiConfig::default();

        let title = Some(String::from("foo"));
        let window_identity = WindowIdentity { title, ..WindowIdentity::default() };
        let new_window_options = WindowOptions { window_identity, ..WindowOptions::default() };
        let options = Options { window_options: new_window_options, ..Options::default() };
        options.override_config(&mut config);

        assert!(!config.window.dynamic_title);
    }

    #[test]
    fn dynamic_title_not_overridden_by_config() {
        let mut config = UiConfig::default();

        config.window.identity.title = "foo".to_owned();
        Options::default().override_config(&mut config);

        assert!(config.window.dynamic_title);
    }

    #[test]
    fn valid_option_as_value() {
        // Test with a single field.
        let value: Value = toml::from_str("field=true").unwrap();

        let mut table = Table::new();
        table.insert(String::from("field"), Value::Boolean(true));

        assert_eq!(value, Value::Table(table));

        // Test with nested fields
        let value: Value = toml::from_str("parent.field=true").unwrap();

        let mut parent_table = Table::new();
        parent_table.insert(String::from("field"), Value::Boolean(true));
        let mut table = Table::new();
        table.insert(String::from("parent"), Value::Table(parent_table));

        assert_eq!(value, Value::Table(table));
    }

    #[test]
    fn invalid_option_as_value() {
        let value = toml::from_str::<Value>("}");
        assert!(value.is_err());
    }

    #[test]
    fn float_option_as_value() {
        let value: Value = toml::from_str("float=3.4").unwrap();

        let mut expected = Table::new();
        expected.insert(String::from("float"), Value::Float(3.4));

        assert_eq!(value, Value::Table(expected));
    }

    #[test]
    fn parse_instance_class() {
        let class = parse_class("one").unwrap();
        assert_eq!(class.general, "one");
        assert_eq!(class.instance, "one");
    }

    #[test]
    fn parse_general_class() {
        let class = parse_class("one,two").unwrap();
        assert_eq!(class.general, "one");
        assert_eq!(class.instance, "two");
    }

    #[test]
    fn parse_invalid_class() {
        let class = parse_class("one,two,three");
        assert!(class.is_err());
    }

    #[test]
    fn valid_decimal() {
        let value = parse_hex_or_decimal("10485773");
        assert_eq!(value, Some(10485773));
    }

    #[test]
    fn valid_hex_to_decimal() {
        let value = parse_hex_or_decimal("0xa0000d");
        assert_eq!(value, Some(10485773));
    }

    #[test]
    fn invalid_hex_to_decimal() {
        let value = parse_hex_or_decimal("0xa0xx0d");
        assert_eq!(value, None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn completions() {
        let mut clap = Options::command();

        for (shell, file) in &[
            (Shell::Bash, "alacritty.bash"),
            (Shell::Fish, "alacritty.fish"),
            (Shell::Zsh, "_alacritty"),
        ] {
            let mut generated = Vec::new();
            clap_complete::generate(*shell, &mut clap, "alacritty", &mut generated);
            let generated = String::from_utf8_lossy(&generated);

            let mut completion = String::new();
            let mut file = File::open(format!("../extra/completions/{}", file)).unwrap();
            file.read_to_string(&mut completion).unwrap();

            assert_eq!(generated, completion);
        }

        // NOTE: Use this to generate new completions.
        //
        // let mut file = File::create("../extra/completions/alacritty.bash").unwrap();
        // clap_complete::generate(Shell::Bash, &mut clap, "alacritty", &mut file);
        // let mut file = File::create("../extra/completions/alacritty.fish").unwrap();
        // clap_complete::generate(Shell::Fish, &mut clap, "alacritty", &mut file);
        // let mut file = File::create("../extra/completions/_alacritty").unwrap();
        // clap_complete::generate(Shell::Zsh, &mut clap, "alacritty", &mut file);
    }
}
