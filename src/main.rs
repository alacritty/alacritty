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
#![cfg_attr(feature = "clippy", plugin(clippy))]
#![cfg_attr(feature = "clippy", feature(plugin))]

#[macro_use]
extern crate alacritty;

#[macro_use]
extern crate log;

use std::error::Error;
use std::sync::Arc;

use alacritty::cli;
use alacritty::config::{self, Config};
use alacritty::display::Display;
use alacritty::event;
use alacritty::event_loop::{self, EventLoop, Msg};
use alacritty::logging;
use alacritty::sync::FairMutex;
use alacritty::term::Term;
use alacritty::tty::{self, process_should_exit};
use alacritty::util::fmt::Red;

fn main() {
    // Load command line options and config
    let options = cli::Options::load();
    let config = load_config(&options);

    // Run alacritty
    if let Err(err) = run(config, options) {
        die!(
            "Alacritty encountered an unrecoverable error:\n\n\t{}\n",
            Red(err)
        );
    }

    info!("Goodbye.");
}

/// Load configuration
///
/// If a configuration file is given as a command line argument we don't
/// generate a default file. If an empty configuration file is given, i.e.
/// /dev/null, we load the compiled-in defaults.
fn load_config(options: &cli::Options) -> Config {
    let config_path = options
        .config_path()
        .or_else(|| Config::installed_config())
        .unwrap_or_else(|| {
            Config::write_defaults()
                .unwrap_or_else(|err| die!("Write defaults config failure: {}", err))
        });

    Config::load_from(&*config_path).unwrap_or_else(|err| match err {
        config::Error::NotFound => {
            die!("Config file not found at: {}", config_path.display());
        }
        config::Error::Empty => {
            eprintln!("Empty config; Loading defaults");
            Config::default()
        }
        _ => die!("{}", err),
    })
}

/// Run Alacritty
///
/// Creates a window, the terminal state, pty, I/O event loop, input processor,
/// config change monitor, and runs the main display loop.
fn run(mut config: Config, options: cli::Options) -> Result<(), Box<Error>> {
    // Initialize the logger first as to capture output from other subsystems
    logging::initialize(&options)?;

    info!("Welcome to Alacritty.");
    config.path().map(|config_path| {
        info!("Configuration loaded from {}", config_path.display());
    });

    // Create a display.
    //
    // The display manages a window and can draw the terminal
    let mut display = Display::new(&config, &options)?;

    info!(
        "PTY Dimensions: {:?} x {:?}",
        display.size().lines(),
        display.size().cols()
    );

    // Create the terminal
    //
    // This object contains all of the state about what's being displayed. It's
    // wrapped in a clonable mutex since both the I/O loop and display need to
    // access it.
    let terminal = Term::new(&config, display.size().to_owned());
    let terminal = Arc::new(FairMutex::new(terminal));

    // Find the window ID for setting $WINDOWID
    let window_id = display.get_window_id();

    // Create the pty
    //
    // The pty forks a process to run the shell on the slave side of the
    // pseudoterminal. A file descriptor for the master side is retained for
    // reading/writing to the shell.
    let mut pty = tty::new(&config, &options, *display.size(), window_id);

    // Get a reference to something that we can resize
    //
    // This exists because the EventedRW interface is not necessarily thread-safe
    // and we need to be able to resize the PTY from the main thread while the IO
    // thread owns the EventedRW object.
    let resize_handle = unsafe { &mut *pty.winpty.get() };

    // Create the pseudoterminal I/O loop
    //
    // pty I/O is ran on another thread as to not occupy cycles used by the
    // renderer and input processing. Note that access to the terminal state is
    // synchronized since the I/O loop updates the state, and the display
    // consumes it periodically.
    let event_loop = EventLoop::new(
        Arc::clone(&terminal),
        display.notifier(),
        pty,
        options.ref_test,
    );

    // The event loop channel allows write requests from the event processor
    // to be sent to the loop and ultimately written to the pty.
    let loop_tx = event_loop.channel();

    // Event processor
    //
    // Need the Rc<RefCell<_>> here since a ref is shared in the resize callback
    let mut processor = event::Processor::new(
        event_loop::Notifier(event_loop.channel()),
        display.resize_channel(),
        &options,
        &config,
        options.ref_test,
        display.size().to_owned(),
    );

    // Create a config monitor when config was loaded from path
    //
    // The monitor watches the config file for changes and reloads it. Pending
    // config changes are processed in the main loop.
    let config_monitor = match (options.live_config_reload, config.live_config_reload()) {
        // Start monitor if CLI flag says yes
        (Some(true), _) |
        // Or if no CLI flag was passed and the config says yes
        (None, true) => config.path()
                .map(|path| config::Monitor::new(path, display.notifier())),
        // Otherwise, don't start the monitor
        _ => None,
    };

    // Kick off the I/O thread
    let io_thread = event_loop.spawn(None);

    // Main display loop
    loop {
        // Process input and window events
        let mut terminal = processor.process_events(&terminal, display.window());

        // Handle config reloads
        config_monitor
            .as_ref()
            .and_then(|monitor| monitor.pending_config())
            .map(|new_config| {
                config = new_config;
                display.update_config(&config);
                processor.update_config(&config);
                terminal.update_config(&config);
                terminal.dirty = true;
            });

        // Maybe draw the terminal
        if terminal.needs_draw() {
            // Try to update the position of the input method editor
            display.update_ime_position(&terminal);
            // Handle pending resize events
            //
            // The second argument is a list of types that want to be notified
            // of display size changes.
            display.handle_resize(&mut terminal, &config, &mut [resize_handle, &mut processor]);

            // Draw the current state of the terminal
            display.draw(terminal, &config, processor.selection.as_ref());
        }

        // Begin shutdown if the flag was raised.
        if process_should_exit() {
            break;
        }
    }

    loop_tx
        .send(Msg::Shutdown)
        .expect("Error sending shutdown to event loop");

    // FIXME patch notify library to have a shutdown method
    // config_reloader.join().ok();

    Ok(())
}
