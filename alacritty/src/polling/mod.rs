//! Unix I/O event polling.

use polling::{Event as PollEvent, Events, Poller};
use std::io::Error as IoError;
use std::path::PathBuf;
use std::process;

use log::error;
use std::result::Result;
use winit::event_loop::EventLoopProxy;

use alacritty_terminal::thread;

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
    signal_listener: SignalListener,
    ipc_listener: IpcListener,
    events: Events,
    poller: Poller,
}

impl IoListener {
    /// Create background thread to listen for I/O events.
    pub fn spawn(
        options: &Options,
        event_proxy: EventLoopProxy<Event>,
    ) -> Result<IoListenerHandle, IoError> {
        let poller = Poller::new()?;
        let events = Events::new();

        // Create socket listener for IPC messages.
        let ipc_socket_path = options.socket.clone().unwrap_or_else(|| {
            let mut path = ipc::socket_dir();
            path.push(format!("{}-{}.sock", ipc::socket_prefix(), process::id()));
            path
        });
        let ipc_listener = IpcListener::new(options, event_proxy.clone(), &ipc_socket_path)?;

        // Create listener for Unix signals.
        let signal_listener = SignalListener::new(event_proxy)?;

        // SAFETY: Correct drop order is taken care of by `Drop` implementation.
        unsafe { poller.add(&signal_listener.pipe, PollEvent::readable(SIGNAL_READ_KEY))? };
        unsafe { poller.add(&ipc_listener.socket, PollEvent::readable(IPC_READ_KEY))? };

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
        self.poller.modify(&self.ipc_listener.socket, PollEvent::readable(IPC_READ_KEY))?;

        // Wait for the next event to be ready.
        self.events.clear();
        self.poller.wait(&mut self.events, None)?;

        for event in self.events.iter() {
            if event.key == IPC_READ_KEY {
                self.ipc_listener.process_message()?;
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
        if let Err(err) = self.poller.delete(&self.ipc_listener.socket) {
            error!("Failed to remove IPC listener interest: {err}");
        }
    }
}

/// Public I/O event listener state.
pub struct IoListenerHandle {
    pub ipc_socket_path: PathBuf,
}
