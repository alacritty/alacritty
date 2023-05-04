//! Logging for Alacritty.
//!
//! The main executable is supposed to call `initialize()` exactly once during
//! startup. All logging messages are written to stdout, given that their
//! log-level is sufficient for the level configured in `cli::Options`.

use std::fs::{File, OpenOptions};
use std::io::{self, LineWriter, Stdout, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::{env, process};

use log::{self, Level, LevelFilter};
use once_cell::sync::Lazy;
use winit::event_loop::EventLoopProxy;

use alacritty_terminal::config::LOG_TARGET_CONFIG;

use crate::cli::Options;
use crate::event::{Event, EventType};
use crate::message_bar::{Message, MessageType};

/// Logging target for IPC config error messages.
pub const LOG_TARGET_IPC_CONFIG: &str = "alacritty_log_ipc_config";

/// Name for the environment variable containing the log file's path.
const ALACRITTY_LOG_ENV: &str = "ALACRITTY_LOG";

/// Name for the environment varibale containing extra logging targets.
///
/// The targets are semicolon separated.
const ALACRITTY_EXTRA_LOG_TARGETS_ENV: &str = "ALACRITTY_EXTRA_LOG_TARGETS";

/// User configurable extra log targets to include.
static EXTRA_LOG_TARGETS: Lazy<Vec<String>> = Lazy::new(|| {
    env::var(ALACRITTY_EXTRA_LOG_TARGETS_ENV)
        .map_or(Vec::new(), |targets| targets.split(';').map(ToString::to_string).collect())
});

/// List of targets which will be logged by Alacritty.
const ALLOWED_TARGETS: &[&str] = &[
    LOG_TARGET_IPC_CONFIG,
    LOG_TARGET_CONFIG,
    "alacritty_config_derive",
    "alacritty_terminal",
    "alacritty",
    "crossfont",
];

pub fn initialize(
    options: &Options,
    event_proxy: EventLoopProxy<Event>,
) -> Result<Option<PathBuf>, log::SetLoggerError> {
    log::set_max_level(options.log_level());

    let logger = Logger::new(event_proxy);
    let path = logger.file_path();
    log::set_boxed_logger(Box::new(logger))?;

    Ok(path)
}

pub struct Logger {
    logfile: Mutex<OnDemandLogFile>,
    stdout: Mutex<LineWriter<Stdout>>,
    event_proxy: Mutex<EventLoopProxy<Event>>,
    start: Instant,
}

impl Logger {
    fn new(event_proxy: EventLoopProxy<Event>) -> Self {
        let logfile = Mutex::new(OnDemandLogFile::new());
        let stdout = Mutex::new(LineWriter::new(io::stdout()));

        Logger { logfile, stdout, event_proxy: Mutex::new(event_proxy), start: Instant::now() }
    }

    fn file_path(&self) -> Option<PathBuf> {
        if let Ok(logfile) = self.logfile.lock() {
            Some(logfile.path().clone())
        } else {
            None
        }
    }

    /// Log a record to the message bar.
    fn message_bar_log(&self, record: &log::Record<'_>, logfile_path: &str) {
        let message_type = match record.level() {
            Level::Error => MessageType::Error,
            Level::Warn => MessageType::Warning,
            _ => return,
        };

        let event_proxy = match self.event_proxy.lock() {
            Ok(event_proxy) => event_proxy,
            Err(_) => return,
        };

        #[cfg(not(windows))]
        let env_var = format!("${}", ALACRITTY_LOG_ENV);
        #[cfg(windows)]
        let env_var = format!("%{}%", ALACRITTY_LOG_ENV);

        let message = format!(
            "[{}] See log at {} ({}):\n{}",
            record.level(),
            logfile_path,
            env_var,
            record.args(),
        );

        let mut message = Message::new(message, message_type);
        message.set_target(record.target().to_owned());

        let _ = event_proxy.send_event(Event::new(EventType::Message(message), None));
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record<'_>) {
        // Get target crate.
        let index = record.target().find(':').unwrap_or_else(|| record.target().len());
        let target = &record.target()[..index];

        // Only log our own crates, except when logging at Level::Trace.
        if !self.enabled(record.metadata()) || !is_allowed_target(record.level(), target) {
            return;
        }

        // Create log message for the given `record` and `target`.
        let message = create_log_message(record, target, self.start);

        if let Ok(mut logfile) = self.logfile.lock() {
            // Write to logfile.
            let _ = logfile.write_all(message.as_ref());

            // Log relevant entries to message bar.
            self.message_bar_log(record, &logfile.path.to_string_lossy());
        }

        // Write to stdout.
        if let Ok(mut stdout) = self.stdout.lock() {
            let _ = stdout.write_all(message.as_ref());
        }
    }

    fn flush(&self) {}
}

fn create_log_message(record: &log::Record<'_>, target: &str, start: Instant) -> String {
    let runtime = start.elapsed();
    let secs = runtime.as_secs();
    let nanos = runtime.subsec_nanos();
    let mut message = format!("[{}.{:0>9}s] [{:<5}] [{}] ", secs, nanos, record.level(), target);

    // Alignment for the lines after the first new line character in the payload. We don't deal
    // with fullwidth/unicode chars here, so just `message.len()` is sufficient.
    let alignment = message.len();

    // Push lines with added extra padding on the next line, which is trimmed later.
    let lines = record.args().to_string();
    for line in lines.split('\n') {
        let line = format!("{}\n{:width$}", line, "", width = alignment);
        message.push_str(&line);
    }

    // Drop extra trailing alignment.
    message.truncate(message.len() - alignment);
    message
}

/// Check if log messages from a crate should be logged.
fn is_allowed_target(level: Level, target: &str) -> bool {
    match (level, log::max_level()) {
        (Level::Error, LevelFilter::Trace) | (Level::Warn, LevelFilter::Trace) => true,
        _ => ALLOWED_TARGETS.contains(&target) || EXTRA_LOG_TARGETS.iter().any(|t| t == target),
    }
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
            let file = OpenOptions::new().append(true).create_new(true).open(&self.path);

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
