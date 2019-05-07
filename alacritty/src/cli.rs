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

use std::path::{Path, PathBuf};
use std::borrow::Cow;

use log;
use clap::{crate_authors, crate_description, crate_name, crate_version, App, Arg};

use alacritty_terminal::config::{Config, Delta, Dimensions, Shell};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::window::DEFAULT_NAME;

/// Options specified on the command line
pub struct Options {
    pub live_config_reload: Option<bool>,
    pub print_events: bool,
    pub ref_test: bool,
    pub dimensions: Option<Dimensions>,
    pub position: Option<Delta<i32>>,
    pub title: Option<String>,
    pub class: Option<String>,
    pub log_level: log::LevelFilter,
    pub command: Option<Shell<'static>>,
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
            log_level: log::LevelFilter::Warn,
            command: None,
            working_dir: None,
            config: None,
            persistent_logging: false,
        }
    }
}

impl Options {
    /// Build `Options` from command line arguments.
    pub fn new() -> Self {
        let mut options = Options::default();

        let version_string = format!("{} ({})",
                                     crate_version!(),
                                     env!("GIT_HASH"));

        let matches = App::new(crate_name!())
            .version(version_string.as_str())
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
                "Specify alternative configuration file [default: \
                 $XDG_CONFIG_HOME/alacritty/alacritty.yml]",
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

        match matches.occurrences_of("q") {
            0 => {},
            1 => options.log_level = log::LevelFilter::Error,
            2 | _ => options.log_level = log::LevelFilter::Off,
        }

        match matches.occurrences_of("v") {
            0 if !options.print_events => {},
            0 | 1 => options.log_level = log::LevelFilter::Info,
            2 => options.log_level = log::LevelFilter::Debug,
            3 | _ => options.log_level = log::LevelFilter::Trace,
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

        options
    }

    pub fn config_path(&self) -> Option<Cow<'_, Path>> {
        self.config.as_ref().map(|p| Cow::Borrowed(p.as_path()))
    }

    pub fn into_config(self, mut config: Config) -> Config {
        config.set_live_config_reload(self.live_config_reload.unwrap_or(config.live_config_reload()));
        config.set_working_directory(self.working_dir.or(config.working_directory().to_owned()));
        config.shell = self.command.or(config.shell);

        config.window.dimensions = self.dimensions.unwrap_or(config.window.dimensions);
        config.window.position = self.position.or(config.window.position);
        config.window.title = self.title.or(config.window.title);
        config.window.class = self.class.or(config.window.class);

        config.set_dynamic_title(config.dynamic_title() && config.window.title.is_none());

        config.debug.persistent_logging = self.persistent_logging || config.debug.persistent_logging;
        config.debug.log_level = std::cmp::max(self.log_level, config.debug.log_level);
        config.debug.print_events = self.print_events || config.debug.print_events;
        config.debug.ref_test = self.ref_test || config.debug.ref_test;

        config
    }
}

#[cfg(test)]
mod test {
    use alacritty_terminal::config::{Config, DEFAULT_ALACRITTY_CONFIG};

    use crate::cli::Options;

    #[test]
    fn dynamic_title_ignoring_options_by_default() {
        let config: Config =
            ::serde_yaml::from_str(DEFAULT_ALACRITTY_CONFIG).expect("deserialize config");
        let old_dynamic_title = config.dynamic_title();

        let config = Options::default().into_config(config);

        assert_eq!(old_dynamic_title, config.dynamic_title());
    }

    #[test]
    fn dynamic_title_overridden_by_options() {
        let config: Config =
            ::serde_yaml::from_str(DEFAULT_ALACRITTY_CONFIG).expect("deserialize config");

        let mut options = Options::default();
        options.title = Some("foo".to_owned());
        let config = options.into_config(config);

        assert!(!config.dynamic_title());
    }

    #[test]
    fn dynamic_title_overridden_by_config() {
        let mut config: Config =
            ::serde_yaml::from_str(DEFAULT_ALACRITTY_CONFIG).expect("deserialize config");

        config.window.title = Some("foo".to_owned());
        let config = Options::default().into_config(config);

        assert!(!config.dynamic_title());
    }
}
