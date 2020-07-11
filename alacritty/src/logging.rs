//! Logging for Alacritty.
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

use glutin::event_loop::EventLoopProxy;
use log::{self, Level};

use alacritty_terminal::term::color;

use crate::cli::Options;
use crate::event::Event;
use crate::message_bar::Message;

const ALACRITTY_LOG_ENV: &str = "ALACRITTY_LOG";

pub fn initialize(
    options: &Options,
    event_proxy: EventLoopProxy<Event>,
) -> Result<Option<PathBuf>, log::SetLoggerError> {
    log::set_max_level(options.log_level);

    let logger = Logger::new(event_proxy);
    let path = logger.file_path();
    log::set_boxed_logger(Box::new(logger))?;

    Ok(path)
}

pub struct Logger {
    logfile: Mutex<OnDemandLogFile>,
    stdout: Mutex<LineWriter<Stdout>>,
    event_proxy: Mutex<EventLoopProxy<Event>>,
}

impl Logger {
    fn new(event_proxy: EventLoopProxy<Event>) -> Self {
        let logfile = Mutex::new(OnDemandLogFile::new());
        let stdout = Mutex::new(LineWriter::new(io::stdout()));

        Logger { logfile, stdout, event_proxy: Mutex::new(event_proxy) }
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
            let now = time::strftime("%F %T.%f", &time::now()).unwrap();

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

                    if let Ok(event_proxy) = self.event_proxy.lock() {
                        let mut message = Message::new(msg, color);
                        message.set_target(record.target().to_owned());

                        let _ = event_proxy.send_event(Event::Message(message));
                    }
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

        // Set log path as an environment variable.
        env::set_var(ALACRITTY_LOG_ENV, path.as_os_str());

        OnDemandLogFile { path, file: None, created: Arc::new(AtomicBool::new(false)) }
    }

    fn file(&mut self) -> Result<&mut LineWriter<File>, io::Error> {
        // Allow to recreate the file if it has been deleted at runtime.
        if self.file.is_some() && !self.path.as_path().exists() {
            self.file = None;
        }

        // Create the file if it doesn't exist yet.
        if self.file.is_none() {
            let file = OpenOptions::new().append(true).create(true).open(&self.path);

            match file {
                Ok(file) => {
                    self.file = Some(io::LineWriter::new(file));
                    self.created.store(true, Ordering::Relaxed);
                    let _ =
                        writeln!(io::stdout(), "Created log file at \"{}\"", self.path.display());
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
