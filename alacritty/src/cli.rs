// Copyright 2019 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::cmp::max;
use std::path::PathBuf;

use clap::{crate_authors, crate_description, crate_name, crate_version, App, Arg};
use log::{self, error, LevelFilter};

use alacritty_terminal::config::{Delta, Dimensions, Shell, DEFAULT_NAME};
use alacritty_terminal::index::{Column, Line};

use crate::config::Config;

#[cfg(not(any(target_os = "macos", windows)))]
const CONFIG_PATH: &str = "$XDG_CONFIG_HOME/alacritty/alacritty.yml";
#[cfg(windows)]
const CONFIG_PATH: &str = "%APPDATA%\\alacritty\\alacritty.yml";
#[cfg(target_os = "macos")]
const CONFIG_PATH: &str = "$HOME/.config/alacritty/alacritty.yml";

/// Options specified on the command line
pub struct Options {
    pub live_config_reload: Option<bool>,
    pub print_events: bool,
    pub ref_test: bool,
    pub dimensions: Option<Dimensions>,
    pub position: Option<Delta<i32>>,
    pub title: Option<String>,
    pub class: Option<String>,
    pub embed: Option<String>,
    pub log_level: LevelFilter,
    pub command: Option<Shell<'static>>,
    pub hold: bool,
    pub working_dir: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub persistent_logging: bool,
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
            class: None,
            embed: None,
            log_level: LevelFilter::Warn,
            command: None,
            hold: false,
            working_dir: None,
            config: None,
            persistent_logging: false,
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
                    .takes_value(true)
                    .help(&format!("Defines window class on Linux [default: {}]", DEFAULT_NAME)),
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
                    .min_values(1)
                    .allow_hyphen_values(true)
                    .help("Command and args to execute (must be last argument)"),
            )
            .arg(Arg::with_name("hold").long("hold").help("Remain open after child process exits"))
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

        options.class = matches.value_of("class").map(ToOwned::to_owned);
        options.title = matches.value_of("title").map(ToOwned::to_owned);
        options.embed = matches.value_of("embed").map(ToOwned::to_owned);

        match matches.occurrences_of("q") {
            0 => {},
            1 => options.log_level = LevelFilter::Error,
            2 | _ => options.log_level = LevelFilter::Off,
        }

        match matches.occurrences_of("v") {
            0 if !options.print_events => options.log_level = LevelFilter::Warn,
            0 | 1 => options.log_level = LevelFilter::Info,
            2 => options.log_level = LevelFilter::Debug,
            3 | _ => options.log_level = LevelFilter::Trace,
        }

        if let Some(dir) = matches.value_of("working-directory") {
            options.working_dir = Some(PathBuf::from(dir.to_string()));
        }

        if let Some(path) = matches.value_of("config-file") {
            options.config = Some(PathBuf::from(path.to_string()));
        }

        if let Some(mut args) = matches.values_of("command") {
            // The following unwrap is guaranteed to succeed.
            // If 'command' exists it must also have a first item since
            // Arg::min_values(1) is set.
            let command = String::from(args.next().unwrap());
            let args = args.map(String::from).collect();
            options.command = Some(Shell::new_with_args(command, args));
        }

        if matches.is_present("hold") {
            options.hold = true;
        }

        options
    }

    pub fn config_path(&self) -> Option<PathBuf> {
        self.config.clone()
    }

    pub fn into_config(self, mut config: Config) -> Config {
        match self.working_dir.or_else(|| config.working_directory.take()) {
            Some(ref wd) if !wd.is_dir() => error!("Unable to set working directory to {:?}", wd),
            wd => config.working_directory = wd,
        }

        if let Some(lcr) = self.live_config_reload {
            config.set_live_config_reload(lcr);
        }
        config.shell = self.command.or(config.shell);

        config.hold = self.hold;

        config.window.dimensions = self.dimensions.unwrap_or(config.window.dimensions);
        config.window.title = self.title.unwrap_or(config.window.title);
        config.window.position = self.position.or(config.window.position);
        config.window.embed = self.embed.and_then(|embed| embed.parse().ok());

        if let Some(class) = self.class {
            let parts: Vec<_> = class.split(',').collect();
            config.window.class.instance = parts[0].into();
            if let Some(&general) = parts.get(1) {
                config.window.class.general = general.into();
            }
        }

        config.set_dynamic_title(config.dynamic_title() && config.window.title == DEFAULT_NAME);

        config.debug.print_events = self.print_events || config.debug.print_events;
        config.debug.log_level = max(config.debug.log_level, self.log_level);
        config.debug.ref_test = self.ref_test || config.debug.ref_test;
        config.debug.persistent_logging =
            self.persistent_logging || config.debug.persistent_logging;

        if config.debug.print_events {
            config.debug.log_level = max(config.debug.log_level, LevelFilter::Info);
        }

        config
    }
}

#[cfg(test)]
mod test {
    use crate::cli::Options;
    use crate::config::Config;

    #[test]
    fn dynamic_title_ignoring_options_by_default() {
        let config = Config::default();
        let old_dynamic_title = config.dynamic_title();

        let config = Options::default().into_config(config);

        assert_eq!(old_dynamic_title, config.dynamic_title());
    }

    #[test]
    fn dynamic_title_overridden_by_options() {
        let config = Config::default();

        let mut options = Options::default();
        options.title = Some("foo".to_owned());
        let config = options.into_config(config);

        assert!(!config.dynamic_title());
    }

    #[test]
    fn dynamic_title_overridden_by_config() {
        let mut config = Config::default();

        config.window.title = "foo".to_owned();
        let config = Options::default().into_config(config);

        assert!(!config.dynamic_title());
    }
}
