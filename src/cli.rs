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
extern crate log;
use clap::{Arg, App};
use index::{Line, Column};
use config::{Dimensions, Shell};

const DEFAULT_TITLE: &'static str = "Alacritty";

/// Options specified on the command line
pub struct Options {
    pub print_events: bool,
    pub ref_test: bool,
    pub dimensions: Option<Dimensions>,
    pub title: String,
    pub log_level: log::LogLevelFilter,
    pub shell: Option<Shell<'static>>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            print_events: false,
            ref_test: false,
            dimensions: None,
            title: DEFAULT_TITLE.to_owned(),
            log_level: log::LogLevelFilter::Warn,
            shell: None,
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
            .arg(Arg::with_name("print-events")
                .long("print-events"))
            .arg(Arg::with_name("dimensions")
                .long("dimensions")
                .short("d")
                .value_names(&["columns", "lines"])
                .help("Defines the window dimensions [default: 80x24]"))
            .arg(Arg::with_name("title")
                .long("title")
                .short("t")
                .default_value(DEFAULT_TITLE)
                .help("Defines the window title"))
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
            .arg(Arg::with_name("command")
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

        if let Some(mut dimensions) = matches.values_of("dimensions") {
            let width = dimensions.next().map(|w| w.parse().map(|w| Column(w)));
            let height = dimensions.next().map(|h| h.parse().map(|h| Line(h)));
            if let (Some(Ok(width)), Some(Ok(height))) = (width, height) {
                options.dimensions = Some(Dimensions::new(width, height));
            }
        }

        if let Some(title) = matches.value_of("title") {
            options.title = title.to_owned();
        }

        match matches.occurrences_of("q") {
            0 => {},
            1 => options.log_level = log::LogLevelFilter::Error,
            2 | _ => options.log_level = log::LogLevelFilter::Off
        }

        match matches.occurrences_of("v") {
            0 => {},
            1 => options.log_level = log::LogLevelFilter::Info,
            2 => options.log_level = log::LogLevelFilter::Debug,
            3 | _ => options.log_level = log::LogLevelFilter::Trace
        }

        if let Some(mut args) = matches.values_of("command") {
            // The following unwrap is guaranteed to succeed.
            // If 'command' exists it must also have a first item since
            // Arg::min_values(1) is set.
            let command = String::from(args.next().unwrap());
            let args = args.map(String::from).collect();
            options.shell = Some(Shell::new_with_args(command, args));
        }

        options
    }

    pub fn dimensions(&self) -> Option<Dimensions> {
        self.dimensions
    }

    pub fn shell(&self) -> Option<&Shell> {
        self.shell.as_ref()
    }
}
