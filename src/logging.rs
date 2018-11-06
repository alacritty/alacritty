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
use cli;
use log::{self, Level};
use tempfile;

use std::fs::File;
use std::io::{self, LineWriter, Stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static ERRORS: AtomicBool = AtomicBool::new(false);
static WARNINGS: AtomicBool = AtomicBool::new(false);

pub fn initialize(options: &cli::Options) -> Result<(), log::SetLoggerError> {
    // Use env_logger if RUST_LOG environment variable is defined. Otherwise,
    // use the alacritty-only logger.
    if ::std::env::var("RUST_LOG").is_ok() {
        ::env_logger::try_init()
    } else {
        log::set_boxed_logger(Box::new(Logger::new(options.log_level)))
    }
}

pub fn warnings() -> bool {
    WARNINGS.load(Ordering::Relaxed)
}

pub fn errors() -> bool {
    ERRORS.load(Ordering::Relaxed)
}

pub fn clear_errors() {
    ERRORS.store(false, Ordering::Relaxed);
}

pub fn clear_warnings() {
    WARNINGS.store(false, Ordering::Relaxed);
}

pub struct Logger {
    level: log::LevelFilter,
    logfile: Mutex<OnDemandTempFile>,
    stdout: Mutex<LineWriter<Stdout>>,
}

impl Logger {
    // False positive, see: https://github.com/rust-lang-nursery/rust-clippy/issues/734
    #[cfg_attr(feature = "cargo-clippy", allow(new_ret_no_self))]
    pub fn new(level: log::LevelFilter) -> Self {
        log::set_max_level(level);

        let logfile = Mutex::new(OnDemandTempFile::new("alacritty", String::from(".log")));

        let stdout = Mutex::new(LineWriter::new(io::stdout()));

        Logger {
            level,
            logfile,
            stdout,
        }
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) && record.target().starts_with("alacritty") {
            if let Ok(ref mut logfile) = self.logfile.lock() {
                let _ = logfile.write_all(format!("{}\n", record.args()).as_ref());
            }

            if let Ok(ref mut stdout) = self.stdout.lock() {
                let _ = stdout.write_all(format!("{}\n", record.args()).as_ref());
            }

            match record.level() {
                Level::Error => ERRORS.store(true, Ordering::Relaxed),
                Level::Warn => WARNINGS.store(true, Ordering::Relaxed),
                _ => (),
            }
        }
    }

    fn flush(&self) {}
}

struct OnDemandTempFile {
    file: Option<LineWriter<File>>,
    prefix: String,
    suffix: String,
}

impl OnDemandTempFile {
    fn new<T: Into<String>, J: Into<String>>(prefix: T, suffix: J) -> Self {
        OnDemandTempFile {
            file: None,
            prefix: prefix.into(),
            suffix: suffix.into(),
        }
    }

    fn file(&mut self) -> Result<&mut LineWriter<File>, io::Error> {
        if self.file.is_none() {
            let file = tempfile::Builder::new()
                .prefix(&self.prefix)
                .suffix(&self.suffix)
                .tempfile()?;
            let path = file.path().to_owned();
            self.file = Some(io::LineWriter::new(file.persist(&path)?));
            println!("Created log file at {:?}", path);
        }

        Ok(self.file.as_mut().unwrap())
    }
}

impl Write for OnDemandTempFile {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        self.file()?.write(buf)
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        self.file()?.flush()
    }
}
