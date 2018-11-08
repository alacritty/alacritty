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

use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, LineWriter, Stdout, Write};
use std::path::PathBuf;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub fn initialize(options: &cli::Options) -> Result<LoggerProxy, log::SetLoggerError> {
    // Use env_logger if RUST_LOG environment variable is defined. Otherwise,
    // use the alacritty-only logger.
    if ::std::env::var("RUST_LOG").is_ok() {
        ::env_logger::try_init()?;
        Ok(LoggerProxy::default())
    } else {
        let logger = Logger::new(options.log_level);
        let proxy = logger.proxy();

        log::set_boxed_logger(Box::new(logger))?;

        Ok(proxy)
    }
}

/// Proxy object for bidirectional communicating with the global logger.
#[derive(Clone, Default)]
pub struct LoggerProxy {
    errors: Arc<AtomicBool>,
    warnings: Arc<AtomicBool>,
    logfile_proxy: OnDemandLogFileProxy,
}

impl LoggerProxy {
    /// Check for new logged errors.
    pub fn errors(&self) -> bool {
        self.errors.load(Ordering::Relaxed)
    }

    /// Check for new logged warnings.
    pub fn warnings(&self) -> bool {
        self.warnings.load(Ordering::Relaxed)
    }

    /// Get the path of the log file if it has been created.
    pub fn log_path(&self) -> Option<&str> {
        if self.logfile_proxy.created.load(Ordering::Relaxed) {
            Some(&self.logfile_proxy.path)
        } else {
            None
        }
    }

    /// Clear log warnings/errors from the Alacritty UI.
    pub fn clear(&mut self) {
        self.errors.store(false, Ordering::Relaxed);
        self.warnings.store(false, Ordering::Relaxed);
    }
}

struct Logger {
    level: log::LevelFilter,
    logfile: Mutex<OnDemandLogFile>,
    stdout: Mutex<LineWriter<Stdout>>,
    errors: Arc<AtomicBool>,
    warnings: Arc<AtomicBool>,
}

impl Logger {
    // False positive, see: https://github.com/rust-lang-nursery/rust-clippy/issues/734
    #[cfg_attr(feature = "cargo-clippy", allow(new_ret_no_self))]
    fn new(level: log::LevelFilter) -> Self {
        log::set_max_level(level);

        let logfile = Mutex::new(OnDemandLogFile::new());
        let stdout = Mutex::new(LineWriter::new(io::stdout()));

        Logger {
            level,
            logfile,
            stdout,
            errors: Arc::new(AtomicBool::new(false)),
            warnings: Arc::new(AtomicBool::new(false)),
        }
    }

    fn proxy(&self) -> LoggerProxy {
        LoggerProxy {
            errors: self.errors.clone(),
            warnings: self.warnings.clone(),
            logfile_proxy: self.logfile.lock().expect("").proxy(),
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
                Level::Error => self.errors.store(true, Ordering::Relaxed),
                Level::Warn => self.warnings.store(true, Ordering::Relaxed),
                _ => (),
            }
        }
    }

    fn flush(&self) {}
}

#[derive(Clone, Default)]
struct OnDemandLogFileProxy {
    created: Arc<AtomicBool>,
    path: String,
}

struct OnDemandLogFile {
    file: Option<LineWriter<File>>,
    created: Arc<AtomicBool>,
    path: PathBuf,
}

impl OnDemandLogFile {
    fn new() -> Self {
        let mut path = env::temp_dir();
        path.push(format!("Alacritty-{}.log", process::id()));

        OnDemandLogFile {
            path,
            file: None,
            created: Arc::new(AtomicBool::new(false)),
        }
    }

    fn file(&mut self) -> Result<&mut LineWriter<File>, io::Error> {
        // Allow to recreate the file if it has been deleted at runtime
        if self.file.is_some() && !self.path.as_path().exists() {
            self.file = None;
        }

        // Create the file if it doesn't exist yet
        if self.file.is_none() {
            let file = OpenOptions::new()
                .append(true)
                .create(true)
                .open(&self.path);

            match file {
                Ok(file) => {
                    self.file = Some(io::LineWriter::new(file));
                    self.created.store(true, Ordering::Relaxed);
                    let _ = writeln!(io::stdout(), "Created log file at {:?}", self.path);
                }
                Err(e) => {
                    let _ = writeln!(io::stdout(), "Unable to create log file: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(self.file.as_mut().unwrap())
    }

    fn proxy(&self) -> OnDemandLogFileProxy {
        OnDemandLogFileProxy {
            created: self.created.clone(),
            path: self.path.to_string_lossy().to_string(),
        }
    }
}

impl Write for OnDemandLogFile {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        self.file()?.write(buf)
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        self.file()?.flush()
    }
}
