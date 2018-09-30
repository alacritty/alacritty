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
#![cfg_attr(feature = "cargo-clippy", deny(clippy, if_not_else, enum_glob_use, wrong_pub_self_convention))]
#![cfg_attr(feature = "nightly", feature(core_intrinsics))]
#![cfg_attr(all(test, feature = "bench"), feature(test))]

#[macro_use]
extern crate alacritty;

#[macro_use]
extern crate log;
#[cfg(target_os = "macos")]
extern crate dirs;
extern crate glutin;
extern crate mio_more;

use std::error::Error;
use std::sync::Arc;
#[cfg(target_os = "macos")]
use std::env;

use glutin::{
    dpi::{LogicalPosition, PhysicalSize},
    Event, EventsLoop, EventsLoopProxy,
};

use alacritty::cli;
use alacritty::config::Config;
use alacritty::display::Display;
use alacritty::event;
use alacritty::event_loop::{self, EventLoop, Msg, WindowNotifier};
#[cfg(target_os = "macos")]
use alacritty::locale;
use alacritty::logging;
use alacritty::sync::FairMutex;
use alacritty::term::{Term};
use alacritty::tty::{self, process_should_exit};
use alacritty::util::fmt::Red;
use alacritty::window::Window;

fn main() {
    // Load command line options and config
    let options = cli::Options::load();
    let config = load_config(&options).update_dynamic_title(&options);

    // Switch to home directory
    #[cfg(target_os = "macos")]
    env::set_current_dir(dirs::home_dir().unwrap()).unwrap();
    // Set locale
    #[cfg(target_os = "macos")]
    locale::set_locale_environment();

    // Run alacritty
    if let Err(err) = run(config, &options) {
        die!("Alacritty encountered an unrecoverable error:\n\n\t{}\n", Red(err));
    }

    info!("Goodbye.");
}

/// Load configuration
///
/// If a configuration file is given as a command line argument we don't
/// generate a default file. If an empty configuration file is given, i.e.
/// /dev/null, we load the compiled-in defaults.
fn load_config(options: &cli::Options) -> Config {
    let config_path = options.config_path()
        .or_else(Config::installed_config)
        .unwrap_or_else(|| {
            Config::write_defaults()
                .unwrap_or_else(|err| die!("Write defaults config failure: {}", err))
        });

    Config::load_from(&*config_path).unwrap_or_else(|err| {
        eprintln!("Error: {}; Loading default config", err);
        Config::default()
    })
}

/// Run Alacritty
///
/// Creates a window, the terminal state, pty, I/O event loop, input processor,
/// config change monitor, and runs the main display loop.
fn run(config: Config, options: &cli::Options) -> Result<(), Box<Error>> {
    // Initialize the logger first as to capture output from other subsystems
    logging::initialize(options)?;

    info!("Welcome to Alacritty.");
    if let Some(config_path) = config.path() {
        info!("Configuration loaded from {}", config_path.display());
    };

    let mut events_loop = EventsLoop::new();

    let mut instances = std::collections::HashMap::new();

    let instance = Instance::new(&events_loop, &config, &options)?;
    let instance2 = Instance::new(&events_loop, &config, &options)?;

    instances.insert(instance.window.get_glutin_window_id(), instance);
    instances.insert(instance2.window.get_glutin_window_id(), instance2);

    // Main display loop
    loop {
        let mut pending_events: Vec<Event> = vec![];

        for (_, instance) in &instances {
            if instance.wait_for_event {
                events_loop.run_forever(|e| {
                    pending_events.push(e);
                    glutin::ControlFlow::Break
                });
                break;
            }
        }

        {

            let mut process = |e| match e {
                Event::WindowEvent { window_id, .. } => {
                    let instance = instances.get_mut(&window_id).unwrap();
                    let mut terminal_lock =
                        instance
                        .processor
                        .process_event(&instance.terminal, &mut instance.window, e);

                    instance.wait_for_event = !terminal_lock.dirty;

                    if terminal_lock.needs_draw() {
                        let (x, y) = instance.display.current_ime_position(&terminal_lock);
                        instance.window.set_ime_spot(LogicalPosition {
                            x: x as f64,
                            y: y as f64,
                        });
                        // Handle pending resize events
                        //
                        // The second argument is a list of types that want to be notified
                        // of display size changes.
                        instance.display.handle_resize(
                            &mut terminal_lock,
                            &config,
                            &mut [
                            &mut instance.pty,
                            &mut instance.processor,
                            &mut instance.window,
                            ],
                            instance.dpr,
                            );

                        drop(terminal_lock);
                    }
                }
                _ => {
                    for (_, instance) in &mut instances {
                        let terminal_lock = instance.processor.process_event(
                            &instance.terminal,
                            &mut instance.window,
                            e.clone(),
                            );
                        instance.wait_for_event = !terminal_lock.dirty;
                    }
                }
            };

            for event in pending_events.drain(..) {
                process(event);
            }

            events_loop.poll_events(process);
        }

        // Begin shutdown if the flag was raised.
        if process_should_exit() {
            break;
        }

        for (_, instance) in &mut instances {
            if instance.terminal.lock().needs_draw() {
                // Draw the current state of the terminal
                instance
                    .display
                    .draw(&instance.terminal, &config, instance.window.is_focused, instance.window.get_glutin_window_id());

                instance.window.swap_buffers().expect("swap buffers");
            }
        }

    }

    for (_, instance) in instances {
        instance
            .loop_tx
            .send(Msg::Shutdown)
            .expect("Error sending shutdown to event loop");
        let _ = instance.io_thread.join();
    }

    Ok(())
}

struct Notifier(EventsLoopProxy);

impl WindowNotifier for Notifier {
    fn notify(&self) {
        self.0.wakeup().unwrap();
    }
}

struct Instance {
    window: Window,
    display: Display,
    terminal: Arc<FairMutex<Term>>,
    processor: event::Processor<event_loop::Notifier>,
    dpr: f64,
    pty: tty::Pty,
    loop_tx: mio_more::channel::Sender<Msg>,
    io_thread: std::thread::JoinHandle<(event_loop::EventLoop<std::fs::File>, event_loop::State)>,
    wait_for_event: bool,
}

impl Instance {
    fn new(
        events_loop: &EventsLoop,
        config: &Config,
        options: &cli::Options,
    ) -> Result<Instance, Box<Error>> {
        let mut window = Window::new(&options, config.window(), &events_loop)?;

        let dpr = window.hidpi_factor();
        info!("device_pixel_ratio: {}", dpr);

        // Create a display.
        //
        // The display manages a window and can draw the terminal
        let display = Display::new(&config, options, dpr)?;

        let size = display.size().to_owned();
        let viewport_size =
            PhysicalSize::new(size.width as f64, size.height as f64).to_logical(dpr);

        info!("set_inner_size: {:?}", viewport_size);
        window.set_inner_size(viewport_size);

        info!("PTY Dimensions: {:?} x {:?}", size.lines(), size.cols());

        // Create the terminal
        //
        // This object contains all of the state about what's being displayed. It's
        // wrapped in a clonable mutex since both the I/O loop and display need to
        // access it.
        let terminal = Term::new(&config, size);
        let terminal = Arc::new(FairMutex::new(terminal));

        // Find the window ID for setting $WINDOWID
        let window_id = window.get_window_id();

        // Create the pty
        //
        // The pty forks a process to run the shell on the slave side of the
        // pseudoterminal. A file descriptor for the master side is retained for
        // reading/writing to the shell.
        let pty = tty::new(&config, options, &display.size(), window_id);

        // Create the pseudoterminal I/O loop
        //
        // pty I/O is ran on another thread as to not occupy cycles used by the
        // renderer and input processing. Note that access to the terminal state is
        // synchronized since the I/O loop updates the state, and the display
        // consumes it periodically.
        let event_loop = EventLoop::new(
            Arc::clone(&terminal),
            Box::new(Notifier(events_loop.create_proxy())),
            pty.reader(),
            options.ref_test,
        );

        // Event processor
        //
        // Need the Rc<RefCell<_>> here since a ref is shared in the resize callback
        let processor = event::Processor::new(
            event_loop::Notifier(event_loop.channel()),
            display.resize_channel(),
            options,
            &config,
            options.ref_test,
            display.size().to_owned(),
        );

        let loop_tx = event_loop.channel();
        let io_thread = event_loop.spawn(None);

        Ok(Instance {
            window,
            display,
            terminal,
            processor,
            dpr,
            pty,
            loop_tx,
            io_thread,
            wait_for_event: true,
        })
    }
}
