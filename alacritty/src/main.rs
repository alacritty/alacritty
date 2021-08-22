//! Alacritty - The GPU Enhanced Terminal.

#![warn(rust_2018_idioms, future_incompatible)]
#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use)]
#![cfg_attr(feature = "cargo-clippy", deny(warnings))]
// With the default subsystem, 'console', windows creates an additional console
// window for the program.
// This is silently ignored on non-windows systems.
// See https://msdn.microsoft.com/en-us/library/4cc7ya5b.aspx for more details.
#![windows_subsystem = "windows"]

#[cfg(not(any(feature = "x11", feature = "wayland", target_os = "macos", windows)))]
compile_error!(r#"at least one of the "x11"/"wayland" features must be enabled"#);

#[cfg(target_os = "macos")]
use std::env;
use std::fs;
use std::io::{self, Write};
#[cfg(unix)]
use std::process;

use glutin::event_loop::EventLoop as GlutinEventLoop;
use log::{error, info};
#[cfg(windows)]
use winapi::um::wincon::{AttachConsole, FreeConsole, ATTACH_PARENT_PROCESS};

use alacritty_terminal::tty;

mod cli;
mod clipboard;
mod config;
mod daemon;
mod display;
mod event;
mod input;
#[cfg(unix)]
mod ipc;
mod logging;
#[cfg(target_os = "macos")]
mod macos;
mod message_bar;
#[cfg(windows)]
mod panic;
mod renderer;
mod scheduler;
mod window_context;

mod gl {
    #![allow(clippy::all)]
    include!(concat!(env!("OUT_DIR"), "/gl_bindings.rs"));
}

#[cfg(unix)]
use crate::cli::{MessageOptions, Options, Subcommands};
use crate::config::{monitor, Config};
use crate::event::{Event, Processor};
#[cfg(unix)]
use crate::ipc::SOCKET_MESSAGE_CREATE_WINDOW;
#[cfg(target_os = "macos")]
use crate::macos::locale;

fn main() {
    #[cfg(windows)]
    panic::attach_handler();

    // When linked with the windows subsystem windows won't automatically attach
    // to the console of the parent process, so we do it explicitly. This fails
    // silently if the parent has no console.
    #[cfg(windows)]
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }

    // Load command line options.
    let options = Options::new();

    #[cfg(unix)]
    match options.subcommands {
        Some(Subcommands::Msg(options)) => msg(options),
        None => alacritty(options),
    }

    #[cfg(not(unix))]
    alacritty(options);
}

/// `msg` subcommand entrypoint.
#[cfg(unix)]
fn msg(options: MessageOptions) {
    match ipc::send_message(options.socket, &SOCKET_MESSAGE_CREATE_WINDOW) {
        Ok(()) => process::exit(0),
        Err(err) => {
            eprintln!("Error: {}", err);
            process::exit(1);
        },
    }
}

/// Run main Alacritty entrypoint.
///
/// Creates a window, the terminal state, PTY, I/O event loop, input processor,
/// config change monitor, and runs the main display loop.
fn alacritty(options: Options) {
    // Setup glutin event loop.
    let window_event_loop = GlutinEventLoop::<Event>::with_user_event();

    // Initialize the logger as soon as possible as to capture output from other subsystems.
    let log_file = logging::initialize(&options, window_event_loop.create_proxy())
        .expect("Unable to initialize logger");

    // Load configuration file.
    let config = config::load(&options);

    // Update the log level from config.
    log::set_max_level(config.ui_config.debug.log_level);

    // Switch to home directory.
    #[cfg(target_os = "macos")]
    env::set_current_dir(dirs::home_dir().unwrap()).unwrap();
    // Set locale.
    #[cfg(target_os = "macos")]
    locale::set_locale_environment();

    // Store if log file should be deleted before moving config.
    let persistent_logging = config.ui_config.debug.persistent_logging;

    info!("Welcome to Alacritty");

    // Log the configuration paths.
    log_config_path(&config);

    // Set environment variables.
    tty::setup_env(&config);

    // Create a config monitor when config was loaded from path.
    //
    // The monitor watches the config file for changes and reloads it. Pending
    // config changes are processed in the main loop.
    if config.ui_config.live_config_reload {
        monitor::watch(config.ui_config.config_paths.clone(), window_event_loop.create_proxy());
    }

    // Create the IPC socket listener.
    #[cfg(unix)]
    let socket_path = if config.ui_config.ipc_socket {
        ipc::spawn_ipc_socket(window_event_loop.create_proxy())
    } else {
        None
    };

    // Event processor.
    let mut processor = Processor::new(config, options, &window_event_loop);

    // Create the first Alacritty window.
    let proxy = window_event_loop.create_proxy();
    if let Err(error) = processor.create_window(&window_event_loop, proxy) {
        error!("Alacritty encountered an unrecoverable error:\n\n\t{}\n", error);
        std::process::exit(1);
    }

    info!("Initialisation complete");

    // Start event loop and block until shutdown.
    processor.run(window_event_loop);

    // This explicit drop is needed for Windows, ConPTY backend. Otherwise a deadlock can occur.
    // The cause:
    //   - Drop for ConPTY will deadlock if the conout pipe has already been dropped
    //   - ConPTY is dropped when the last of processor and window context are dropped, because both
    //     of them own an Arc<ConPTY>
    //
    // The fix is to ensure that processor is dropped first. That way, when window context (i.e.
    // PTY) is dropped, it can ensure ConPTY is dropped before the conout pipe in the PTY drop
    // order.
    //
    // FIXME: Change PTY API to enforce the correct drop order with the typesystem.
    drop(processor);

    // FIXME patch notify library to have a shutdown method.
    // config_reloader.join().ok();

    // Without explicitly detaching the console cmd won't redraw it's prompt.
    #[cfg(windows)]
    unsafe {
        FreeConsole();
    }

    // Clean up the IPC socket file.
    #[cfg(unix)]
    if let Some(socket_path) = socket_path {
        let _ = fs::remove_file(socket_path);
    }

    info!("Goodbye");

    // Clean up logfile.
    if let Some(log_file) = log_file {
        if !persistent_logging && fs::remove_file(&log_file).is_ok() {
            let _ = writeln!(io::stdout(), "Deleted log file at \"{}\"", log_file.display());
        }
    }
}

fn log_config_path(config: &Config) {
    if config.ui_config.config_paths.is_empty() {
        return;
    }

    let mut msg = String::from("Configuration files loaded from:");
    for path in &config.ui_config.config_paths {
        msg.push_str(&format!("\n  {:?}", path.display()));
    }

    info!("{}", msg);
}
