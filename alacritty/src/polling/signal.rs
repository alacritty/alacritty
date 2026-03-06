//! Unix signal listener.

use std::io::{Error as IoError, Read};
use std::os::unix::net::UnixStream;

use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::low_level::pipe;
use winit::event_loop::EventLoopProxy;

use crate::event::{Event, EventType};

pub struct SignalListener {
    pub pipe: UnixStream,

    event_proxy: EventLoopProxy<Event>,
}

impl SignalListener {
    pub fn new(event_proxy: EventLoopProxy<Event>) -> Result<Self, IoError> {
        let (pipe, write) = UnixStream::pair()?;
        pipe::register(SIGINT, write.try_clone()?)?;
        pipe::register(SIGTERM, write)?;
        Ok(Self { event_proxy, pipe })
    }

    /// Process the next signal.
    pub fn process_signal(&mut self) -> Result<(), IoError> {
        // Submit shutdown request to the main event loop.
        let event = Event::new(EventType::Shutdown, None);
        let _ = self.event_proxy.send_event(event);

        // Ensure signal is drained from pipe.
        self.pipe.read_exact(&mut [0])?;

        Ok(())
    }
}
