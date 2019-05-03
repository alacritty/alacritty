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

use ::log;

use crate::config::{Delta, Dimensions, Shell};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

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
    pub fn dimensions(&self) -> Option<Dimensions> {
        self.dimensions
    }

    pub fn position(&self) -> Option<Delta<i32>> {
        self.position
    }

    pub fn command(&self) -> Option<&Shell<'_>> {
        self.command.as_ref()
    }

    pub fn config_path(&self) -> Option<Cow<'_, Path>> {
        self.config.as_ref().map(|p| Cow::Borrowed(p.as_path()))
    }
}
