//! Unix I/O event polling.

use polling::{Event as PollEvent, Events, Poller};
use std::io::Error as IoError;
use std::path::PathBuf;
use std::process;

use log::error;
use std::result::Result;
use winit::event_loop::EventLoopProxy;

use alacritty_terminal::thread;

use crate::UiConfig;
use crate::cli::Options;
use crate::event::Event;
use crate::polling::ipc::IpcListener;
use crate::polling::signal::SignalListener;

pub mod ipc;
mod signal;

/// Polling key for signal read events.
const SIGNAL_READ_KEY: usize = 1;

/// Polling key for IPC read events.
const IPC_READ_KEY: usize = 0;

/// Unix I/O event listener.
pub struct IoListener {
    ipc_listener: Option<IpcListener>,
    signal_listener: SignalListener,
    events: Events,
    poller: Poller,
}

impl IoListener {
    /// Create background thread to listen for I/O events.
    pub fn spawn(
        config: &UiConfig,
        options: &Options,
        event_proxy: EventLoopProxy<Event>,
    ) -> Result<IoListenerHandle, IoError> {
        let poller = Poller::new()?;
        let events = Events::new();

        // Create socket listener for IPC messages.
        let (ipc_socket_path, ipc_listener) = if config.ipc_socket() {
            let ipc_socket_path = options.socket.clone().unwrap_or_else(|| {
                let mut path = ipc::socket_dir();
                path.push(format!("{}-{}.sock", ipc::socket_prefix(), process::id()));
                path
            });
            let ipc_listener = IpcListener::new(options, event_proxy.clone(), &ipc_socket_path)?;
            (Some(ipc_socket_path), Some(ipc_listener))
        } else {
            (None, None)
        };

        // Create listener for Unix signals.
        let signal_listener = SignalListener::new(event_proxy)?;

        // SAFETY: Correct drop order is taken care of by `Drop` implementation.
        unsafe { poller.add(&signal_listener.pipe, PollEvent::readable(SIGNAL_READ_KEY))? };
        if let Some(ipc_listener) = &ipc_listener {
            unsafe { poller.add(&ipc_listener.socket, PollEvent::readable(IPC_READ_KEY))? };
        }

        let mut listener = Self { signal_listener, ipc_listener, events, poller };

        thread::spawn_named("io event listener", move || {
            loop {
                if let Err(err) = listener.poll() {
                    error!("Failed to poll for I/O events: {err}");
                }
            }
        });

        Ok(IoListenerHandle { ipc_socket_path })
    }

    /// Process the next I/O event.
    fn poll(&mut self) -> Result<(), IoError> {
        // Ensure interests are present for the next poll.
        self.poller.modify(&self.signal_listener.pipe, PollEvent::readable(SIGNAL_READ_KEY))?;
        if let Some(ipc_listener) = &self.ipc_listener {
            self.poller.modify(&ipc_listener.socket, PollEvent::readable(IPC_READ_KEY))?;
        }

        // Wait for the next event to be ready.
        self.events.clear();
        self.poller.wait(&mut self.events, None)?;

        for event in self.events.iter() {
            if event.key == IPC_READ_KEY
                && let Some(ipc_listener) = &mut self.ipc_listener
            {
                ipc_listener.process_message()?;
            } else if event.key == SIGNAL_READ_KEY {
                self.signal_listener.process_signal()?;
            }
        }

        Ok(())
    }
}

impl Drop for IoListener {
    fn drop(&mut self) {
        if let Err(err) = self.poller.delete(&self.signal_listener.pipe) {
            error!("Failed to remove signal listener interest: {err}");
        }
        if let Some(ipc_listener) = &self.ipc_listener
            && let Err(err) = self.poller.delete(&ipc_listener.socket)
        {
            error!("Failed to remove IPC listener interest: {err}");
        }
    }
}

/// Public I/O event listener state.
pub struct IoListenerHandle {
    pub ipc_socket_path: Option<PathBuf>,
}
