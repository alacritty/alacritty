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
use alacritty::event_loop::{self, EventLoop};
use alacritty::logging;
use alacritty::sync::FairMutex;
use alacritty::term::{Term};
use alacritty::tty::{self, process_should_exit};
use alacritty::util::fmt::Red;

fn main() {

    // Load configuration
    let config = Config::load().unwrap_or_else(|err| {
        match err {
            // Use default config when not found
            config::Error::NotFound => {
                match Config::write_defaults() {
                    Ok(path) => err_println!("Config file not found; write defaults config to {:?}", path),
                    Err(err) => err_println!("Write defaults config failure: {}", err)
                }

                Config::load().unwrap()
            },

            // If there's a problem with the config file, print an error
            // and exit.
            _ => die!("{}", err),
        }
    });

    // Load command line options
    let options = cli::Options::load();

    // Run alacritty
    if let Err(err) = run(config, options) {
        die!("Alacritty encountered an unrecoverable error:\n\n\t{}\n", Red(err));
    }

    info!("Goodbye.");
}


/// Run Alacritty
///
/// Creates a window, the terminal state, pty, I/O event loop, input processor,
/// config change monitor, and runs the main display loop.
fn run(mut config: Config, options: cli::Options) -> Result<(), Box<Error>> {
    // Initialize the logger first as to capture output from other subsystems
    logging::initialize(&options)?;

    info!("Welcome to Alacritty.");

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

    // Create the pty
    //
    // The pty forks a process to run the shell on the slave side of the
    // pseudoterminal. A file descriptor for the master side is retained for
    // reading/writing to the shell.
    let mut pty = tty::new(&config, &options, display.size());

    // Create the pseudoterminal I/O loop
    //
    // pty I/O is ran on another thread as to not occupy cycles used by the
    // renderer and input processing. Note that access to the terminal state is
    // synchronized since the I/O loop updates the state, and the display
    // consumes it periodically.
    let event_loop = EventLoop::new(
        terminal.clone(),
        display.notifier(),
        pty.reader(),
        options.ref_test,
    );

    // The event loop channel allows write requests from the event processor
    // to be sent to the loop and ultimately written to the pty.
    let loop_tx = event_loop.channel();

    // Event processor
    //
    // Need the Rc<RefCell<_>> here since a ref is shared in the resize callback
    let mut processor = event::Processor::new(
        event_loop::Notifier(loop_tx),
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
    let config_monitor = config.path()
        .map(|path| config::Monitor::new(path, display.notifier()));

    // Kick off the I/O thread
    let io_thread = event_loop.spawn(None);

    // Main display loop
    loop {
        // Process input and window events
        let mut terminal = processor.process_events(&terminal, display.window());

        // Handle config reloads
        config_monitor.as_ref()
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
            // Handle pending resize events
            //
            // The second argument is a list of types that want to be notified
            // of display size changes.
            display.handle_resize(&mut terminal, &mut [&mut pty, &mut processor], &config, &options)?;

            // Draw the current state of the terminal
            display.draw(terminal, &config, &processor.selection);
        }

        // Begin shutdown if the flag was raised.
        if process_should_exit() {
            break;
        }
    }

    // FIXME patch notify library to have a shutdown method
    // config_reloader.join().ok();

    // Wait for the I/O thread thread to finish
    let _ = io_thread.join();

    Ok(())
}
