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
#![feature(question_mark)]
#![feature(inclusive_range_syntax)]
#![feature(drop_types_in_const)]
#![allow(stable_features)] // lying about question_mark because 1.14.0 isn't released!

#![feature(proc_macro)]

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate alacritty;
extern crate cgmath;
extern crate copypasta;
extern crate errno;
extern crate font;
extern crate glutin;
extern crate libc;
extern crate mio;
extern crate notify;
extern crate parking_lot;
extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
extern crate vte;

#[macro_use]
extern crate bitflags;

use std::error::Error;
use std::sync::{mpsc, Arc};
use std::rc::Rc;

use alacritty::cli;
use alacritty::config::{self, Config};
use alacritty::display::{self, Display};
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
/// Currently, these operations take place in this order.
/// 1.  create a window
/// 2.  create font rasterizer
/// 3.  create quad renderer
/// 4.  create glyph cache
/// 5.  resize window/renderer using info from glyph cache / rasterizer
/// 6.  create a pty
/// 7.  create the terminal
/// 8.  set resize callback on the window
/// 9.  create event loop
/// 10. create display
/// 11. create input processor
/// 12. create config reloader
/// 13. enter main loop
///
/// Observations:
/// * The window + quad renderer + glyph cache and display are closely
///   related Actually, probably include the input processor as well.
///   The resize callback can be lumped in there and that resize step.
///   Rasterizer as well. Maybe we can lump *all* of this into the
///   `Display`.
/// * the pty and event loop closely related
/// * The term bridges the display and pty
/// * Main loop currently manages input, config reload events, drawing, and
///   exiting
///
/// It would be *really* great if this could read more like
///
/// ```ignore
/// let display = Display::new(args..);
/// let pty = Pty::new(display.size());
/// let term = Arc::new(Term::new(display.size());
/// let io_loop = Loop::new(Pty::new(display.size()), term.clone());
/// let config_reloader = config::Monitor::new(&config);
///
/// loop {
///     force_draw = false;
///     // Wait for something to happen
///     processor.process_events(&display);
///
///     // Handle config reloads
///     if let Ok(config) = config_rx.try_recv() {
///         force_draw = true;
///         display.update_config(&config);
///         processor.update_config(&config);
///     }
///
///     // Maybe draw the terminal
///     let terminal = terminal.lock();
///     signal_flag.set(false);
///     if force_draw || terminal.dirty {
///         display.draw(terminal, &config);
///         drop(terminal);
///         display.swap_buffers();
///     }
///
///     // Begin shutdown if the flag was raised.
///     if process_should_exit() {
///         break;
///     }
/// }
/// ```
///
/// instead of the 200 line monster it currently is.
///
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
    let pty = Rc::new(tty::new(display.size()));

    // When the display is resized, inform the kernel of changes to pty
    // dimensions.
    //
    // TODO: The Rc on pty is needed due to a borrowck error here. The borrow
    // checker says that `pty` is still borrowed when it is dropped at the end
    // of the `run` function.
    let pty_ref = pty.clone();
    display.set_resize_callback(move |size| {
        pty_ref.resize(size);
    });

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

    let loop_tx = event_loop.channel();
    let event_loop_handle = event_loop.spawn(None);

    // Event processor
    let resize_tx = display.resize_channel();
    let mut processor = event::Processor::new(
        input::LoopNotifier(loop_tx),
        terminal.clone(),
        resize_tx,
        &config,
        options.ref_test,
    );

    let (config_tx, config_rx) = mpsc::channel();

    // create a config watcher when config is loaded from disk
    let _config_reloader = config.path().map(|path| {
        config::Watcher::new(path, ConfigHandler {
            tx: config_tx,
            loop_kicker: display.notifier(),
        })
    });

    // Main loop
    let mut config_updated = false;
    loop {
        // Wait for something to happen
        processor.process_events(display.window());

        // Handle config reloads
        if let Ok(config) = config_rx.try_recv() {
            config_updated = true;
            display.update_config(&config);
            processor.update_config(&config);
        }

        // Maybe draw the terminal
        let terminal = terminal.lock();
        if terminal.dirty || config_updated {
            display.draw(terminal, &config);
        }

        // Begin shutdown if the flag was raised.
        if process_should_exit() {
            break;
        }
    }

    // FIXME need file watcher to work with custom delegates before
    //       joining config reloader is possible
    //
    // HELP I don't know what I meant in the above fixme
    // config_reloader.join().ok();

    // shutdown
    event_loop_handle.join().ok();

    Ok(())
}

struct ConfigHandler {
    tx: mpsc::Sender<config::Config>,
    loop_kicker: display::Notifier,
}

impl config::OnConfigReload for ConfigHandler {
    fn on_config_reload(&mut self, config: Config) {
        if let Err(..) = self.tx.send(config) {
            err_println!("Failed to notify of new config");
            return;
        }

        self.loop_kicker.notify();
    }
}

