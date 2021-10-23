//! Alacritty socket IPC.

use std::ffi::OsStr;
use std::io::{Error as IoError, ErrorKind, Result as IoResult};
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;
use std::{env, fs, process};

use glutin::event_loop::EventLoopProxy;
use log::warn;

use alacritty_terminal::thread;

use crate::cli::Options;
use crate::event::{Event, EventType};

/// IPC socket message for creating a new window.
pub const SOCKET_MESSAGE_CREATE_WINDOW: [u8; 1] = [1];

/// Environment variable name for the IPC socket path.
const ALACRITTY_SOCKET_ENV: &str = "ALACRITTY_SOCKET";

/// Create an IPC socket.
pub fn spawn_ipc_socket(options: &Options, event_proxy: EventLoopProxy<Event>) -> Option<PathBuf> {
    // Create the IPC socket and export its path as env variable if necessary.
    let socket_path = options.socket.clone().unwrap_or_else(|| {
        let mut path = socket_dir();
        path.push(format!("{}-{}.sock", socket_prefix(), process::id()));
        path
    });
    env::set_var(ALACRITTY_SOCKET_ENV, socket_path.as_os_str());

    let socket = match UnixDatagram::bind(&socket_path) {
        Ok(socket) => socket,
        Err(err) => {
            warn!("Unable to create socket: {:?}", err);
            return None;
        },
    };

    // Spawn a thread to listen on the IPC socket.
    thread::spawn_named("socket listener", move || {
        // Accept up to 2 bytes to ensure only one byte is received.
        // This ensures forward-compatibility.
        let mut buf = [0; 2];

        while let Ok(received) = socket.recv(&mut buf) {
            if buf[..received] == SOCKET_MESSAGE_CREATE_WINDOW {
                let _ = event_proxy.send_event(Event::new(EventType::CreateWindow, None));
            }
        }
    });

    Some(socket_path)
}

/// Send a message to the active Alacritty socket.
pub fn send_message(socket: Option<PathBuf>, message: &[u8]) -> IoResult<()> {
    let socket = find_socket(socket)?;
    socket.send(message)?;
    Ok(())
}

/// Directory for the IPC socket file.
#[cfg(not(target_os = "macos"))]
fn socket_dir() -> PathBuf {
    xdg::BaseDirectories::with_prefix("alacritty")
        .ok()
        .and_then(|xdg| xdg.get_runtime_directory().map(ToOwned::to_owned).ok())
        .and_then(|path| fs::create_dir_all(&path).map(|_| path).ok())
        .unwrap_or_else(env::temp_dir)
}

/// Directory for the IPC socket file.
#[cfg(target_os = "macos")]
fn socket_dir() -> PathBuf {
    env::temp_dir()
}

/// Find the IPC socket path.
fn find_socket(socket_path: Option<PathBuf>) -> IoResult<UnixDatagram> {
    let socket = UnixDatagram::unbound()?;

    // Handle --socket CLI override.
    if let Some(socket_path) = socket_path {
        // Ensure we inform the user about an invalid path.
        socket.connect(&socket_path).map_err(|err| {
            let message = format!("invalid socket path {:?}", socket_path);
            IoError::new(err.kind(), message)
        })?;
    }

    // Handle environment variable.
    if let Ok(path) = env::var(ALACRITTY_SOCKET_ENV) {
        let socket_path = PathBuf::from(path);
        if socket.connect(&socket_path).is_ok() {
            return Ok(socket);
        }
    }

    // Search for sockets files.
    for entry in fs::read_dir(socket_dir())?.filter_map(|entry| entry.ok()) {
        let path = entry.path();

        // Skip files that aren't Alacritty sockets.
        let socket_prefix = socket_prefix();
        if path
            .file_name()
            .and_then(OsStr::to_str)
            .filter(|file| file.starts_with(&socket_prefix) && file.ends_with(".sock"))
            .is_none()
        {
            continue;
        }

        // Attempt to connect to the socket.
        match socket.connect(&path) {
            Ok(_) => return Ok(socket),
            // Delete orphan sockets.
            Err(error) if error.kind() == ErrorKind::ConnectionRefused => {
                let _ = fs::remove_file(&path);
            },
            // Ignore other errors like permission issues.
            Err(_) => (),
        }
    }

    Err(IoError::new(ErrorKind::NotFound, "no socket found"))
}

/// File prefix matching all available sockets.
///
/// This prefix will include display server information to allow for environments with multiple
/// display servers running for the same user.
#[cfg(not(target_os = "macos"))]
fn socket_prefix() -> String {
    let display = env::var("WAYLAND_DISPLAY").or_else(|_| env::var("DISPLAY")).unwrap_or_default();
    format!("Alacritty-{}", display)
}

/// File prefix matching all available sockets.
#[cfg(target_os = "macos")]
fn socket_prefix() -> String {
    String::from("Alacritty")
}
