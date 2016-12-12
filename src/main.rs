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
#![allow(stable_features)] // lying about question_mark because 1.14.0 isn't released!

#[macro_use]
extern crate alacritty;

use std::error::Error;
use std::sync::Arc;

use alacritty::cli;
use alacritty::config::{self, Config};
use alacritty::display::Display;
use alacritty::event;
use alacritty::event_loop::EventLoop;
use alacritty::input;
use alacritty::sync::FairMutex;
use alacritty::term::{Term};
use alacritty::tty::{self, process_should_exit};

fn main() {
    // Load configuration
    let config = match Config::load() {
        // Error loading config
        Err(err) => match err {
            // Use default config when not found
            config::Error::NotFound => {
                err_println!("Config file not found; using defaults");
                Config::default()
            },

            // If there's a problem with the config file, print an error
            // and exit.
            _ => die!("{}", err),
        },

        // Successfully loaded config from file
        Ok(config) => config
    };

    // Load command line options
    let options = cli::Options::load();

    // Run alacritty
    if let Err(err) = run(config, options) {
        die!("{}", err);
    }

    println!("Goodbye");
}

/// Run Alacritty
///
/// Creates a window, the terminal state, pty, I/O event loop, input processor,
/// config change monitor, and runs the main display loop.
fn run(config: Config, options: cli::Options) -> Result<(), Box<Error>> {
    // Create a display.
    //
    // The display manages a window and can draw the terminal
    let mut display = Display::new(&config, &options)?;

    // Create the terminal
    //
    // This object contains all of the state about what's being displayed. It's
    // wrapped in a clonable mutex since both the I/O loop and display need to
    // access it.
    let terminal = Arc::new(FairMutex::new(Term::new(display.size().to_owned())));

    // Create the pty
    //
    // The pty forks a process to run the shell on the slave side of the
    // pseudoterminal. A file descriptor for the master side is retained for
    // reading/writing to the shell.
    let mut pty = tty::new(display.size());

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
        input::LoopNotifier(loop_tx),
        terminal.clone(),
        display.resize_channel(),
        &config,
        options.ref_test,
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
        let wakeup_request = processor.process_events(display.window());

        // Handle config reloads
        let config_updated = config_monitor.as_ref()
            .and_then(|monitor| monitor.pending_config())
            .map(|config| {
                display.update_config(&config);
                processor.update_config(&config);
                true
            }).unwrap_or(false);

        // Maybe draw the terminal
        let mut terminal = terminal.lock();
        if wakeup_request || config_updated {
            // Handle pending resize events
            //
            // The second argument is a list of types that want to be notified
            // of display size changes.
            display.handle_resize(&mut terminal, &mut [&mut pty, &mut processor]);

            // Draw the current state of the terminal
            display.draw(terminal, &config);
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
