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
//
//! Logging for alacritty.
//!
//! The main executable is supposed to call `initialize()` exactly once during
//! startup. All logging messages are written to stdout, given that their
//! log-level is sufficient for the level configured in `cli::Options`.
use log;
use std::sync;
use std::io;
use cli;

pub struct Logger<T> {
    level: log::LevelFilter,
    output: sync::Mutex<T>
}

impl<T: Send + io::Write> Logger<T> {
    // False positive, see: https://github.com/rust-lang-nursery/rust-clippy/issues/734
    #[cfg_attr(feature = "clippy", allow(new_ret_no_self))]
    pub fn new(output: T, level: log::LevelFilter) -> Logger<io::LineWriter<T>> {
        log::set_max_level(level);
        Logger {
            level,
            output: sync::Mutex::new(io::LineWriter::new(output))
        }
    }
}

impl<T: Send + io::Write> log::Log for Logger<T> {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) && record.target().starts_with("alacritty") {
            if let Ok(ref mut writer) = self.output.lock() {
                writer.write_all(format!("{}\n", record.args()).as_ref()).expect("Error while logging!");
            }
        }
    }

    fn flush(&self) {}
}

pub fn initialize(options: &cli::Options) -> Result<(), log::SetLoggerError> {
    // Use env_logger if RUST_LOG environment variable is defined. Otherwise,
    // use the alacritty-only logger.
    if ::std::env::var("RUST_LOG").is_ok() {
        ::env_logger::try_init()
    } else {
        log::set_boxed_logger(Box::new(Logger::new(io::stdout(), options.log_level)))
    }
}
