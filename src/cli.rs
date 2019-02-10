// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
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
use ::log;
use clap::{Arg, App, crate_name, crate_version, crate_authors, crate_description};

use crate::index::{Line, Column};
use crate::config::{Dimensions, Shell};
use crate::window::{DEFAULT_NAME};
use std::path::{Path, PathBuf};
use std::borrow::Cow;

/// Options specified on the command line
pub struct Options {
    pub live_config_reload: Option<bool>,
    pub print_events: bool,
    pub ref_test: bool,
    pub dimensions: Option<Dimensions>,
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
    /// Build `Options` from command line arguments
    pub fn load() -> Options {
        let mut options = Options::default();

        let matches = App::new(crate_name!())
            .version(crate_version!())
            .author(crate_authors!("\n"))
            .about(crate_description!())
            .arg(Arg::with_name("ref-test")
                .long("ref-test")
                .help("Generates ref test"))
            .arg(Arg::with_name("live-config-reload")
                .long("live-config-reload")
                .help("Enable automatic config reloading"))
            .arg(Arg::with_name("no-live-config-reload")
                 .long("no-live-config-reload")
                 .help("Disable automatic config reloading")
                 .conflicts_with("live-config-reload"))
            .arg(Arg::with_name("print-events")
                .long("print-events"))
            .arg(Arg::with_name("persistent-logging")
                .long("persistent-logging")
                .help("Keep the log file after quitting Alacritty"))
            .arg(Arg::with_name("dimensions")
                .long("dimensions")
                .short("d")
                .value_names(&["columns", "lines"])
                .help("Defines the window dimensions. Falls back to size specified by \
                       window manager if set to 0x0 [default: 0x0]"))
            .arg(Arg::with_name("title")
                .long("title")
                .short("t")
                .takes_value(true)
                .help(&format!("Defines the window title [default: {}]", DEFAULT_NAME)))
            .arg(Arg::with_name("class")
                 .long("class")
                 .takes_value(true)
                 .help(&format!("Defines window class on X11 [default: {}]", DEFAULT_NAME)))
            .arg(Arg::with_name("q")
                .short("q")
                .multiple(true)
                .conflicts_with("v")
                .help("Reduces the level of verbosity (the min level is -qq)"))
            .arg(Arg::with_name("v")
                .short("v")
                .multiple(true)
                .conflicts_with("q")
                .help("Increases the level of verbosity (the max level is -vvv)"))
            .arg(Arg::with_name("working-directory")
                 .long("working-directory")
                 .takes_value(true)
                 .help("Start the shell in the specified working directory"))
            .arg(Arg::with_name("config-file")
                 .long("config-file")
                 .takes_value(true)
                 .help("Specify alternative configuration file \
                       [default: $XDG_CONFIG_HOME/alacritty/alacritty.yml]"))
            .arg(Arg::with_name("command")
                .long("command")
                .short("e")
                .multiple(true)
                .takes_value(true)
                .min_values(1)
                .allow_hyphen_values(true)
                .help("Command and args to execute (must be last argument)"))
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

        options.class = matches.value_of("class").map(|c| c.to_owned());
        options.title = matches.value_of("title").map(|t| t.to_owned());

        match matches.occurrences_of("q") {
            0 => {},
            1 => options.log_level = log::LevelFilter::Error,
            2 | _ => options.log_level = log::LevelFilter::Off
        }

        match matches.occurrences_of("v") {
            0 if !options.print_events => {},
            0 | 1 => options.log_level = log::LevelFilter::Info,
            2 => options.log_level = log::LevelFilter::Debug,
            3 | _ => options.log_level = log::LevelFilter::Trace
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

    pub fn dimensions(&self) -> Option<Dimensions> {
        self.dimensions
    }

    pub fn command(&self) -> Option<&Shell<'_>> {
        self.command.as_ref()
    }

    pub fn config_path(&self) -> Option<Cow<'_, Path>> {
        self.config.as_ref().map(|p| Cow::Borrowed(p.as_path()))
    }
}
