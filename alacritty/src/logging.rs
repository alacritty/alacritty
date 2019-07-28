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
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, LineWriter, Stdout, Write};
use std::path::PathBuf;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam_channel::Sender;
use log::{self, Level};
use time;

use alacritty_terminal::message_bar::Message;
use alacritty_terminal::term::color;

use crate::cli::Options;

const ALACRITTY_LOG_ENV: &str = "ALACRITTY_LOG";

pub fn initialize(
    options: &Options,
    message_tx: Sender<Message>,
) -> Result<Option<PathBuf>, log::SetLoggerError> {
    log::set_max_level(options.log_level);

    // Use env_logger if RUST_LOG environment variable is defined. Otherwise,
    // use the alacritty-only logger.
    if ::std::env::var("RUST_LOG").is_ok() {
        ::env_logger::try_init()?;
        Ok(None)
    } else {
        let logger = Logger::new(message_tx);
        let path = logger.file_path();
        log::set_boxed_logger(Box::new(logger))?;
        Ok(path)
    }
}

pub struct Logger {
    logfile: Mutex<OnDemandLogFile>,
    stdout: Mutex<LineWriter<Stdout>>,
    message_tx: Sender<Message>,
}

impl Logger {
    fn new(message_tx: Sender<Message>) -> Self {
        let logfile = Mutex::new(OnDemandLogFile::new());
        let stdout = Mutex::new(LineWriter::new(io::stdout()));

        Logger { logfile, stdout, message_tx }
    }

    fn file_path(&self) -> Option<PathBuf> {
        if let Ok(logfile) = self.logfile.lock() {
            Some(logfile.path().clone())
        } else {
            None
        }
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) && record.target().starts_with("alacritty") {
            let now = time::strftime("%F %R", &time::now()).unwrap();

            let msg = if record.level() >= Level::Trace {
                format!(
                    "[{}] [{}] [{}:{}] {}\n",
                    now,
                    record.level(),
                    record.file().unwrap_or("?"),
                    record.line().map(|l| l.to_string()).unwrap_or_else(|| "?".into()),
                    record.args()
                )
            } else {
                format!("[{}] [{}] {}\n", now, record.level(), record.args())
            };

            if let Ok(ref mut logfile) = self.logfile.lock() {
                let _ = logfile.write_all(msg.as_ref());

                if record.level() <= Level::Warn {
                    #[cfg(not(windows))]
                    let env_var = format!("${}", ALACRITTY_LOG_ENV);
                    #[cfg(windows)]
                    let env_var = format!("%{}%", ALACRITTY_LOG_ENV);

                    let msg = format!(
                        "[{}] See log at {} ({}):\n{}",
                        record.level(),
                        logfile.path.to_string_lossy(),
                        env_var,
                        record.args(),
                    );
                    let color = match record.level() {
                        Level::Error => color::RED,
                        Level::Warn => color::YELLOW,
                        _ => unreachable!(),
                    };

                    let mut message = Message::new(msg, color);
                    message.set_topic(record.file().unwrap_or("?").into());
                    let _ = self.message_tx.send(message);
                }
            }

            if let Ok(ref mut stdout) = self.stdout.lock() {
                let _ = stdout.write_all(msg.as_ref());
            }
        }
    }

    fn flush(&self) {}
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

        // Set log path as an environment variable
        env::set_var(ALACRITTY_LOG_ENV, path.as_os_str());

        OnDemandLogFile { path, file: None, created: Arc::new(AtomicBool::new(false)) }
    }

    fn file(&mut self) -> Result<&mut LineWriter<File>, io::Error> {
        // Allow to recreate the file if it has been deleted at runtime
        if self.file.is_some() && !self.path.as_path().exists() {
            self.file = None;
        }

        // Create the file if it doesn't exist yet
        if self.file.is_none() {
            let file = OpenOptions::new().append(true).create(true).open(&self.path);

            match file {
                Ok(file) => {
                    self.file = Some(io::LineWriter::new(file));
                    self.created.store(true, Ordering::Relaxed);
                    let _ = writeln!(io::stdout(), "Created log file at {:?}", self.path);
                },
                Err(e) => {
                    let _ = writeln!(io::stdout(), "Unable to create log file: {}", e);
                    return Err(e);
                },
            }
        }

        Ok(self.file.as_mut().unwrap())
    }

    fn path(&self) -> &PathBuf {
        &self.path
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
