/// Types to determine the appropriate PTY backend at runtime.
///
/// Unless the winpty feature is disabled, the PTY backend will automatically fall back to
/// WinPTY when the newer ConPTY API is not supported, as long as the user hasn't explicitly
/// opted into the WinPTY config option.
use std::io::{self, Read, Write};

use log::info;
use mio::{Evented, Poll, PollOpt, Ready, Token};
use mio_anonymous_pipes::{EventedAnonRead, EventedAnonWrite};
use mio_named_pipes::NamedPipe;

use crate::config::Config;
use crate::event::OnResize;
use crate::term::SizeInfo;

use super::{conpty, winpty, Pty};

pub fn new<C>(config: &Config<C>, size: &SizeInfo, window_id: Option<usize>) -> Pty {
    if let Some(pty) = conpty::new(config, size, window_id) {
        info!("Using ConPTY backend");
        pty
    } else {
        info!("Using WinPTY backend");
        winpty::new(config, size, window_id)
    }
}

pub enum PtyBackend {
    Winpty(winpty::Agent),
    Conpty(conpty::Conpty),
}

impl OnResize for PtyBackend {
    fn on_resize(&mut self, size: &SizeInfo) {
        match self {
            PtyBackend::Winpty(w) => w.on_resize(size),
            PtyBackend::Conpty(c) => c.on_resize(size),
        }
    }
}

// TODO: The ConPTY API currently must use synchronous pipes as the input
// and output handles. This has led to the need to support two different
// types of pipe.
//
// When https://github.com/Microsoft/console/issues/262 lands then the
// Anonymous variant of this enum can be removed from the codebase and
// everything can just use NamedPipe.
pub enum EventedReadablePipe {
    Anonymous(EventedAnonRead),
    Named(NamedPipe),
}

pub enum EventedWritablePipe {
    Anonymous(EventedAnonWrite),
    Named(NamedPipe),
}

impl Evented for EventedReadablePipe {
    fn register(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        match self {
            EventedReadablePipe::Anonymous(p) => p.register(poll, token, interest, opts),
            EventedReadablePipe::Named(p) => p.register(poll, token, interest, opts),
        }
    }

    fn reregister(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        match self {
            EventedReadablePipe::Anonymous(p) => p.reregister(poll, token, interest, opts),
            EventedReadablePipe::Named(p) => p.reregister(poll, token, interest, opts),
        }
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        match self {
            EventedReadablePipe::Anonymous(p) => p.deregister(poll),
            EventedReadablePipe::Named(p) => p.deregister(poll),
        }
    }
}

impl Read for EventedReadablePipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            EventedReadablePipe::Anonymous(p) => p.read(buf),
            EventedReadablePipe::Named(p) => p.read(buf),
        }
    }
}

impl Evented for EventedWritablePipe {
    fn register(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        match self {
            EventedWritablePipe::Anonymous(p) => p.register(poll, token, interest, opts),
            EventedWritablePipe::Named(p) => p.register(poll, token, interest, opts),
        }
    }

    fn reregister(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        match self {
            EventedWritablePipe::Anonymous(p) => p.reregister(poll, token, interest, opts),
            EventedWritablePipe::Named(p) => p.reregister(poll, token, interest, opts),
        }
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        match self {
            EventedWritablePipe::Anonymous(p) => p.deregister(poll),
            EventedWritablePipe::Named(p) => p.deregister(poll),
        }
    }
}

impl Write for EventedWritablePipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            EventedWritablePipe::Anonymous(p) => p.write(buf),
            EventedWritablePipe::Named(p) => p.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            EventedWritablePipe::Anonymous(p) => p.flush(),
            EventedWritablePipe::Named(p) => p.flush(),
        }
    }
}

impl From<winpty::Agent> for PtyBackend {
    fn from(inner: winpty::Agent) -> Self {
        PtyBackend::Winpty(inner)
    }
}

impl From<conpty::Conpty> for PtyBackend {
    fn from(inner: conpty::Conpty) -> Self {
        PtyBackend::Conpty(inner)
    }
}

impl From<EventedAnonRead> for EventedReadablePipe {
    fn from(inner: EventedAnonRead) -> Self {
        EventedReadablePipe::Anonymous(inner)
    }
}

impl From<NamedPipe> for EventedReadablePipe {
    fn from(inner: NamedPipe) -> Self {
        EventedReadablePipe::Named(inner)
    }
}

impl From<EventedAnonWrite> for EventedWritablePipe {
    fn from(inner: EventedAnonWrite) -> Self {
        EventedWritablePipe::Anonymous(inner)
    }
}

impl From<NamedPipe> for EventedWritablePipe {
    fn from(inner: NamedPipe) -> Self {
        EventedWritablePipe::Named(inner)
    }
}
