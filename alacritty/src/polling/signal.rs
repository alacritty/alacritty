//! Unix signal listener.

use std::io::{Error as IoError, Read};
use std::mem::MaybeUninit;
use std::os::unix::net::UnixStream;
use std::ptr;

use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
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
        pipe::register(SIGTERM, write.try_clone()?)?;
        if !is_ignored(SIGHUP) {
            pipe::register(SIGHUP, write)?;
        }

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

/// Check a signal's ignore disposition.
///
/// Returns true when the given signal is ignored by this process. A signal whose disposition
/// cannot be queried is reported as not ignored.
fn is_ignored(signal: libc::c_int) -> bool {
    let mut action = MaybeUninit::<libc::sigaction>::uninit();

    // SAFETY: `libc::sigaction` is an FFI declaration, so the contract comes from sigaction(2). A
    // NULL `act` installs no action and leaves the current disposition unmodified, `oact` receives
    // that disposition, and a rejected signal number is reported through the return value without
    // writing to `oact`.
    let queried = unsafe { libc::sigaction(signal, ptr::null(), action.as_mut_ptr()) };

    if queried != 0 {
        return false;
    }

    // SAFETY: `sigaction` returned success, so it wrote a whole `struct sigaction` to `action`.
    let action = unsafe { action.assume_init() };

    action.sa_sigaction == libc::SIG_IGN
}
