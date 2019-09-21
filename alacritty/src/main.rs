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
//! Alacritty - The GPU Enhanced Terminal
#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use, clippy::wrong_pub_self_convention)]
#![cfg_attr(feature = "nightly", feature(core_intrinsics))]
#![cfg_attr(all(test, feature = "bench"), feature(test))]
// With the default subsystem, 'console', windows creates an additional console
// window for the program.
// This is silently ignored on non-windows systems.
// See https://msdn.microsoft.com/en-us/library/4cc7ya5b.aspx for more details.
#![windows_subsystem = "windows"]

use futures::sync::mpsc as futures_mpsc;
use futures::sync::oneshot;
#[cfg(target_os = "macos")]
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, Write};
#[cfg(not(windows))]
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

#[cfg(target_os = "macos")]
use dirs;
use log::{debug, error, info};
use serde_json as json;
#[cfg(windows)]
use winapi::um::wincon::{AttachConsole, FreeConsole, ATTACH_PARENT_PROCESS};

use alacritty_charts::SizeInfo;
use alacritty_charts::TimeSeriesSource;
use alacritty_terminal::clipboard::Clipboard;
use alacritty_terminal::config::{Config, Monitor};
use alacritty_terminal::display::Display;
use alacritty_terminal::event_loop::{self, EventLoop, Msg};
#[cfg(target_os = "macos")]
use alacritty_terminal::locale;
use alacritty_terminal::message_bar::MessageBuffer;
use alacritty_terminal::panic;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{cell::Cell, Term};
use alacritty_terminal::tty;
use alacritty_terminal::util::fmt::Red;
use alacritty_terminal::{die, event};
use futures::future::lazy;
use futures::sync::mpsc;
use tokio::prelude::*;
use tokio::runtime::Runtime;

mod cli;
mod config;
mod logging;

use crate::cli::Options;

fn main() {
    panic::attach_handler();

    // When linked with the windows subsystem windows won't automatically attach
    // to the console of the parent process, so we do it explicitly. This fails
    // silently if the parent has no console.
    #[cfg(windows)]
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }

    // Load command line options
    let options = Options::new();

    // Setup storage for message UI
    let message_buffer = MessageBuffer::new();

    // Initialize the logger as soon as possible as to capture output from other subsystems
    let log_file =
        logging::initialize(&options, message_buffer.tx()).expect("Unable to initialize logger");

    // Load configuration file
    // If the file is a command line argument, we won't write a generated default file
    let config_path = options
        .config_path()
        .or_else(config::installed_config)
        .or_else(|| config::write_defaults().ok())
        .map(|path| path.to_path_buf());
    let config = if let Some(path) = config_path {
        config::load_from(path)
    } else {
        error!("Unable to write the default config");
        Config::default()
    };
    let config = options.into_config(config);

    // Update the log level from config
    log::set_max_level(config.debug.log_level);

    // Switch to home directory
    #[cfg(target_os = "macos")]
    env::set_current_dir(dirs::home_dir().unwrap()).unwrap();
    // Set locale
    #[cfg(target_os = "macos")]
    locale::set_locale_environment();

    // Store if log file should be deleted before moving config
    let persistent_logging = config.persistent_logging();

    // Run alacritty
    if let Err(err) = run(config, message_buffer) {
        die!("Alacritty encountered an unrecoverable error:\n\n\t{}\n", Red(err));
    }

    // Clean up logfile
    if let Some(log_file) = log_file {
        if !persistent_logging && fs::remove_file(&log_file).is_ok() {
            let _ = writeln!(io::stdout(), "Deleted log file at {:?}", log_file);
        }
    }
}

/// Sends a request to the async_coordinator to get the latest update epoch of all
/// the charts
fn get_last_updated_chart_epoch(
    charts_tx: futures_mpsc::Sender<alacritty_charts::async_utils::AsyncChartTask>,
) -> u64 {
    let (chart_tx, chart_rx) = oneshot::channel();
    let get_latest_update_epoch = charts_tx
        .send(alacritty_charts::async_utils::AsyncChartTask::SendLastUpdatedEpoch(chart_tx))
        .map_err(|e| error!("Sending SendLastUpdatedEpoch Task: err={:?}", e))
        .and_then(move |_res| {
            debug!("Sent Request for SendLastUpdatedEpoch");
            Ok(())
        });
    tokio::spawn(lazy(move || get_latest_update_epoch));
    let chart_rx = chart_rx.map(|x| x);
    match chart_rx.wait() {
        Ok(data) => {
            debug!("Got response from SendLastUpdatedEpoch Task: {:?}", data);
            data
        },
        Err(err) => {
            error!("Error response from SendLastUpdatedEpoch Task: {:?}", err);
            0u64
        },
    }
}

/// Run Alacritty
///
/// Creates a window, the terminal state, pty, I/O event loop, input processor,
/// config change monitor, and runs the main display loop.
fn run(config: Config, message_buffer: MessageBuffer) -> Result<(), Box<dyn Error>> {
    info!("Welcome to Alacritty");
    if let Some(config_path) = &config.config_path {
        info!("Configuration loaded from {:?}", config_path.display());
    };

    let charts = config.charts.clone();
    let mut chart_index = 0usize;
    // Create the channel that is used to communicate with the
    // background task.
    let (charts_tx, charts_rx) = mpsc::channel(4_096usize);
    let charts_poll_tx = charts_tx.clone();
    // Set environment variables
    tty::setup_env(&config);

    let mut runtime = Runtime::new().expect("failed to start new tokio Runtime");
    tokio::run(lazy(move || {
        // Create a display.
        // The display manages a window and can draw the terminal
        let mut display = match Display::new(&config) {
            Ok(display) => display,
            Err(err) => {
                error!("Got error creating display: {}", err);
                return Err(());
            },
        };

        info!("PTY Dimensions: {:?} x {:?}", display.size().lines(), display.size().cols());

        // Create new native clipboard
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let clipboard = Clipboard::new(display.get_wayland_display());
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let clipboard = Clipboard::new();

        // Create the terminal
        //
        // This object contains all of the state about what's being displayed. It's
        // wrapped in a clonable mutex since both the I/O loop and display need to
        // access it.
        let terminal = Term::new(&config, display.size().to_owned(), message_buffer, clipboard);
        let terminal = Arc::new(FairMutex::new(terminal));

        // Find the window ID for setting $WINDOWID
        let window_id = display.get_window_id();

        // Create the pty
        //
        // The pty forks a process to run the shell on the slave side of the
        // pseudoterminal. A file descriptor for the master side is retained for
        // reading/writing to the shell.
        let pty = tty::new(&config, &display.size(), window_id);

        // Get a reference to something that we can resize
        //
        // This exists because rust doesn't know the interface is thread-safe
        // and we need to be able to resize the PTY from the main thread while the IO
        // thread owns the EventedRW object.
        #[cfg(windows)]
        let mut resize_handle = pty.resize_handle();
        #[cfg(not(windows))]
        let mut resize_handle = pty.fd.as_raw_fd();

        // Create the pseudoterminal I/O loop
        //
        // pty I/O is ran on another thread as to not occupy cycles used by the
        // renderer and input processing. Note that access to the terminal state is
        // synchronized since the I/O loop updates the state, and the display
        // consumes it periodically.
        let event_loop =
            EventLoop::new(Arc::clone(&terminal), display.notifier(), pty, config.debug.ref_test);

        // The event loop channel allows write requests from the event processor
        // to be sent to the loop and ultimately written to the pty.
        let loop_tx = event_loop.channel();

        // Event processor
        //
        // Need the Rc<RefCell<_>> here since a ref is shared in the resize callback
        let mut processor = event::Processor::new(
            event_loop::Notifier(event_loop.channel()),
            display.resize_channel(),
            &config,
            display.size().to_owned(),
        );

        // Create a config monitor when config was loaded from path
        //
        // The monitor watches the config file for changes and reloads it. Pending
        // config changes are processed in the main loop.
        let config_monitor = if config.live_config_reload() {
            config.config_path.as_ref().map(|path| Monitor::new(path, display.notifier()))
        } else {
            None
        };

        // Kick off the I/O thread
        let _io_thread = event_loop.spawn(None);

        info!("Initialisation complete");
        // Copy the terminal size into the alacritty_charts SizeInfo copy.
        let charts_size_info = alacritty_charts::SizeInfo {
            height: display.size().height,
            width: display.size().width,
            padding_y: display.size().padding_y,
            padding_x: display.size().padding_x,
            ..alacritty_charts::SizeInfo::default()
        };
        runtime.spawn(lazy(move || {
            alacritty_charts::async_utils::async_coordinator(
                charts_rx,
                charts,
                charts_size_info.height,
                charts_size_info.width,
                charts_size_info.padding_y,
                charts_size_info.padding_x,
            )
        }));
        let mut chart_last_drawn = 0; // Keep an epoch for the last time we drew the charts
        for chart in config.charts.clone() {
            debug!("Loading chart series with name: '{}'", chart.name);
            let mut series_index = 0usize;
            for series in chart.sources {
                if let TimeSeriesSource::PrometheusTimeSeries(ref prom) = series {
                    debug!(" - Found time_series, adding interval run");
                    let data_request = alacritty_charts::async_utils::MetricRequest {
                        source_url: prom.source.clone(),
                        pull_interval: prom.pull_interval as u64,
                        chart_index,
                        series_index,
                        capacity: prom.series.metrics_capacity,
                        data: None,
                    };
                    let charts_poll_tx = charts_poll_tx.clone();
                    runtime.spawn(lazy(move || {
                        alacritty_charts::async_utils::spawn_interval_polls(
                            &data_request,
                            charts_poll_tx,
                        )
                    }));
                }
                series_index += 1;
            }
            chart_index += 1;
        }

        // Main display loop
        loop {
            // Process input and window events
            let mut terminal_lock = processor.process_events(&terminal, display.window());

            // Handle config reloads
            if let Some(ref path) = config_monitor.as_ref().and_then(Monitor::pending) {
                // Clear old config messages from bar
                terminal_lock.message_buffer_mut().remove_topic(config::SOURCE_FILE_PATH);

                if let Ok(config) = config::reload_from(path) {
                    display.update_config(&config);
                    processor.update_config(&config);
                    terminal_lock.update_config(&config);
                }

                terminal_lock.dirty = true;
            }

            // Begin shutdown if the flag was raised
            if terminal_lock.should_exit() || tty::process_should_exit() {
                break;
            }

            // Maybe draw the terminal
            if terminal_lock.needs_draw()
                || chart_last_drawn != get_last_updated_chart_epoch(charts_tx.clone())
            {
                // Try to update the position of the input method editor
                #[cfg(not(windows))]
                display.update_ime_position(&terminal_lock);

                // Handle pending resize events
                //
                // The second argument is a list of types that want to be notified
                // of display size changes.
                display.handle_resize(
                    &mut terminal_lock,
                    &config,
                    &mut resize_handle,
                    &mut processor,
                    charts_tx.clone(),
                );

                drop(terminal_lock);

                // Draw the current state of the terminal
                display.draw(&terminal, &config, charts_tx.clone());
                chart_last_drawn =
                    std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            }
        }
        runtime.shutdown_now().wait().expect("Unable to wait for shutdown");
        info!("Out of the loop!");

        // Write ref tests to disk
        if config.debug.ref_test {
            write_ref_test_results(&terminal.lock());
        }

        loop_tx.send(Msg::Shutdown).expect("Error sending shutdown to event loop");
        Ok(())
    }));

    // FIXME patch notify library to have a shutdown method
    // config_reloader.join().ok();

    // Without explicitly detaching the console cmd won't redraw it's prompt
    #[cfg(windows)]
    unsafe {
        FreeConsole();
    }

    info!("Goodbye");

    Ok(())
}

// Write the ref test results to the disk
fn write_ref_test_results(terminal: &Term) {
    // dump grid state
    let mut grid = terminal.grid().clone();
    grid.initialize_all(&Cell::default());
    grid.truncate();

    let serialized_grid = json::to_string(&grid).expect("serialize grid");

    let serialized_size = json::to_string(terminal.size_info()).expect("serialize size");

    let serialized_config = format!("{{\"history_size\":{}}}", grid.history_size());

    File::create("./grid.json")
        .and_then(|mut f| f.write_all(serialized_grid.as_bytes()))
        .expect("write grid.json");

    File::create("./size.json")
        .and_then(|mut f| f.write_all(serialized_size.as_bytes()))
        .expect("write size.json");

    File::create("./config.json")
        .and_then(|mut f| f.write_all(serialized_config.as_bytes()))
        .expect("write config.json");
}
