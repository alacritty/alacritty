//! Alacritty socket IPC.

use std::ffi::OsStr;
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;
use std::{env, fs, io, process};

use glutin::event_loop::EventLoopProxy;

use alacritty_terminal::thread;

use crate::event::{Event, EventType};

/// IPC socket message for creating a new window.
pub const SOCKET_MESSAGE_CREATE_WINDOW: [u8; 1] = [1];

/// Environment variable name for the IPC socket path.
const ALACRITTY_SOCKET_ENV: &str = "ALACRITTY_SOCKET";

/// Create an IPC socket.
pub fn spawn_ipc_socket(event_proxy: EventLoopProxy<Event>) -> Option<PathBuf> {
    // Create the IPC socket and export its path as env variable if necessary.
    let socket_path = match env::var(ALACRITTY_SOCKET_ENV) {
        Ok(path) => PathBuf::from(path),
        Err(_) => {
            let mut path = env::temp_dir();
            path.push(format!("Alacritty-{}.sock", process::id()));
            env::set_var(ALACRITTY_SOCKET_ENV, path.as_os_str());
            path
        },
    };

    let socket = match UnixDatagram::bind(&socket_path) {
        Ok(socket) => socket,
        Err(_) => return None,
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
pub fn send_message(message: &[u8]) -> io::Result<()> {
    let socket_path = find_socket_path()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no socket found"))?;

    let socket = UnixDatagram::unbound()?;
    socket.connect(&socket_path)?;
    socket.send(message)?;

    Ok(())
}

/// Find the IPC socket path.
fn find_socket_path() -> Option<PathBuf> {
    // Try the environment variable.
    if let Ok(path) = env::var(ALACRITTY_SOCKET_ENV) {
        let socket_path = PathBuf::from(path);
        if socket_path.exists() {
            return Some(socket_path);
        }
    }

    // Search for sockets in /tmp.
    for entry in fs::read_dir(env::temp_dir()).ok()? {
        let path = entry.ok()?.path();
        if let Some(file) = path.file_name().and_then(OsStr::to_str) {
            if file.starts_with("Alacritty-") && file.ends_with(".sock") {
                return Some(path);
            }
        }
    }

    None
}
