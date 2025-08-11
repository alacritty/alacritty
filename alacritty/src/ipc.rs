//! Alacritty socket IPC.

use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Error as IoError, ErrorKind, Result as IoResult, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::{env, fs, process};

use log::{error, warn};
use std::result::Result;
use winit::event_loop::EventLoopProxy;
use winit::window::WindowId;

use alacritty_terminal::thread;

use crate::cli::{Options, SocketMessage};
use crate::event::{Event, EventType};

/// Environment variable name for the IPC socket path.
const ALACRITTY_SOCKET_ENV: &str = "ALACRITTY_SOCKET";

/// Create an IPC socket.
pub fn spawn_ipc_socket(
    options: &Options,
    event_proxy: EventLoopProxy<Event>,
) -> IoResult<PathBuf> {
    // Create the IPC socket and export its path as env.

    let socket_path = options.socket.clone().unwrap_or_else(|| {
        let mut path = socket_dir();
        path.push(format!("{}-{}.sock", socket_prefix(), process::id()));
        path
    });

    let listener = UnixListener::bind(&socket_path)?;

    unsafe { env::set_var(ALACRITTY_SOCKET_ENV, socket_path.as_os_str()) };
    if options.daemon {
        println!("ALACRITTY_SOCKET={}; export ALACRITTY_SOCKET", socket_path.display());
    }

    // Spawn a thread to listen on the IPC socket.
    thread::spawn_named("socket listener", move || {
        let mut data = String::new();
        for stream in listener.incoming().filter_map(Result::ok) {
            data.clear();
            let mut reader = BufReader::new(&stream);

            match reader.read_line(&mut data) {
                Ok(0) | Err(_) => continue,
                Ok(_) => (),
            };

            // Read pending events on socket.
            let message: SocketMessage = match serde_json::from_str(&data) {
                Ok(message) => message,
                Err(err) => {
                    warn!("Failed to convert data from socket: {err}");
                    continue;
                },
            };

            // Handle IPC events.
            match message {
                SocketMessage::CreateWindow(options) => {
                    let event = Event::new(EventType::CreateWindow(options), None);
                    let _ = event_proxy.send_event(event);
                },
                SocketMessage::Config(ipc_config) => {
                    let window_id = ipc_config
                        .window_id
                        .and_then(|id| u64::try_from(id).ok())
                        .map(WindowId::from);
                    let event = Event::new(EventType::IpcConfig(ipc_config), window_id);
                    let _ = event_proxy.send_event(event);
                },
                SocketMessage::GetConfig(config) => {
                    let window_id =
                        config.window_id.and_then(|id| u64::try_from(id).ok()).map(WindowId::from);
                    let event = Event::new(EventType::IpcGetConfig(Arc::new(stream)), window_id);
                    let _ = event_proxy.send_event(event);
                },
            }
        }
    });

    Ok(socket_path)
}

/// Send a message to the active Alacritty socket.
pub fn send_message(socket: Option<PathBuf>, message: SocketMessage) -> IoResult<()> {
    let mut socket = find_socket(socket)?;

    // Write message to socket.
    let message_json = serde_json::to_string(&message)?;
    socket.write_all(message_json.as_bytes())?;
    let _ = socket.flush();

    // Shutdown write end, to allow reading.
    socket.shutdown(Shutdown::Write)?;

    // Get matching IPC reply.
    handle_reply(&socket, &message)?;

    Ok(())
}

/// Process IPC responses.
fn handle_reply(stream: &UnixStream, message: &SocketMessage) -> IoResult<()> {
    // Read reply, returning early if there is none.
    let mut buffer = String::new();
    let mut reader = BufReader::new(stream);
    if let Ok(0) | Err(_) = reader.read_line(&mut buffer) {
        return Ok(());
    }

    // Parse IPC reply.
    let reply: SocketReply = serde_json::from_str(&buffer)
        .map_err(|err| IoError::other(format!("Invalid IPC format: {err}")))?;

    // Ensure reply matches request.
    match (message, &reply) {
        // Write requested config to STDOUT.
        (SocketMessage::GetConfig(..), SocketReply::GetConfig(config)) => {
            println!("{config}");
            Ok(())
        },
        // Ignore requests without reply.
        _ => Ok(()),
    }
}

/// Send IPC message reply.
pub fn send_reply(stream: &mut UnixStream, message: SocketReply) {
    if let Err(err) = send_reply_fallible(stream, message) {
        error!("Failed to send IPC reply: {err}");
    }
}

/// Send IPC message reply, returning possible errors.
fn send_reply_fallible(stream: &mut UnixStream, message: SocketReply) -> IoResult<()> {
    let json = serde_json::to_string(&message).map_err(IoError::other)?;
    stream.write_all(json.as_bytes())?;
    stream.flush()?;
    Ok(())
}

/// Directory for the IPC socket file.
#[cfg(not(target_os = "macos"))]
fn socket_dir() -> PathBuf {
    xdg::BaseDirectories::with_prefix("alacritty")
        .get_runtime_directory()
        .map(ToOwned::to_owned)
        .ok()
        .and_then(|path| fs::create_dir_all(&path).map(|_| path).ok())
        .unwrap_or_else(env::temp_dir)
}

/// Directory for the IPC socket file.
#[cfg(target_os = "macos")]
fn socket_dir() -> PathBuf {
    env::temp_dir()
}

/// Find the IPC socket path.
fn find_socket(socket_path: Option<PathBuf>) -> IoResult<UnixStream> {
    // Handle --socket CLI override.
    if let Some(socket_path) = socket_path {
        // Ensure we inform the user about an invalid path.
        return UnixStream::connect(&socket_path).map_err(|err| {
            let message = format!("invalid socket path {socket_path:?}");
            IoError::new(err.kind(), message)
        });
    }

    // Handle environment variable.
    if let Ok(path) = env::var(ALACRITTY_SOCKET_ENV) {
        let socket_path = PathBuf::from(path);
        if let Ok(socket) = UnixStream::connect(socket_path) {
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
        match UnixStream::connect(&path) {
            Ok(socket) => return Ok(socket),
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
    format!("Alacritty-{}", display.replace('/', "-"))
}

/// File prefix matching all available sockets.
#[cfg(target_os = "macos")]
fn socket_prefix() -> String {
    String::from("Alacritty")
}

/// IPC socket replies.
#[derive(Serialize, Deserialize, Debug)]
pub enum SocketReply {
    GetConfig(String),
}
