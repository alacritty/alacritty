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
use std::env;
use index::{Line, Column};

/// Options specified on the command line
pub struct Options {
    pub print_events: bool,
    pub ref_test: bool,
    pub columns: Column,
    pub lines: Line,
    pub title: String
}

impl Default for Options {
    fn default() -> Options {
        Options {
            print_events: false,
            ref_test: false,
            columns: Column(80),
            lines: Line(24),
            title: "Alacritty".to_owned()
        }
    }
}

impl Options {
    /// Iterate through env::args() to build `Options`
    pub fn load() -> Options {
        let mut options = Options::default();
        let mut args_iter = env::args();

        while let Some(arg) = args_iter.next() {
            match &arg[..] {
                // Generate ref test
                "--ref-test" => options.ref_test = true,
                "--print-events" => options.print_events = true,
                // Set dimensions
                "-d" | "--dimensions" => {
                    args_iter.next()
                        .map(|w| w.parse().map(|w| options.columns = Column(w)));
                    args_iter.next()
                        .map(|h| h.parse().map(|h| options.lines = Line(h)));
                },
                "-t" | "--title" => {
                    args_iter.next().map(|t| options.title = t);
                },
                // ignore unexpected
                _ => (),
            }
        }

        options
    }

    pub fn lines_u32(&self) -> u32 {
        self.lines.0 as u32
    }

    pub fn columns_u32(&self) -> u32 {
        self.columns.0 as u32
    }
}
