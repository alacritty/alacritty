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

use std::ffi::OsStr;
use std::io;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::sync::mpsc::TryRecvError;

use crate::config::{Config, Program};
use crate::event::OnResize;
use crate::term::SizeInfo;
use crate::tty::windows::child::ChildExitWatcher;
use crate::tty::{ChildEvent, EventedPty, EventedReadWrite};

#[cfg(feature = "winpty")]
mod automatic_backend;
mod child;
mod conpty;
#[cfg(feature = "winpty")]
mod winpty;

#[cfg(not(feature = "winpty"))]
use conpty::Conpty as Backend;
#[cfg(not(feature = "winpty"))]
use mio_anonymous_pipes::{EventedAnonRead as ReadPipe, EventedAnonWrite as WritePipe};

#[cfg(feature = "winpty")]
use automatic_backend::{
    EventedReadablePipe as ReadPipe, EventedWritablePipe as WritePipe, PtyBackend as Backend,
};

pub struct Pty {
    // XXX: Backend is required to be the first field, to ensure correct drop order. Dropping
    // `conout` before `backend` will cause a deadlock (with Conpty).
    backend: Backend,
    conout: ReadPipe,
    conin: WritePipe,
    read_token: mio::Token,
    write_token: mio::Token,
    child_event_token: mio::Token,
    child_watcher: ChildExitWatcher,
}

#[cfg(not(feature = "winpty"))]
pub fn new<C>(config: &Config<C>, size: &SizeInfo, window_id: Option<usize>) -> Pty {
    conpty::new(config, size, window_id).expect("Failed to create ConPTY backend")
}

#[cfg(feature = "winpty")]
pub fn new<C>(config: &Config<C>, size: &SizeInfo, window_id: Option<usize>) -> Pty {
    automatic_backend::new(config, size, window_id)
}

impl Pty {
    fn new(
        backend: impl Into<Backend>,
        conout: impl Into<ReadPipe>,
        conin: impl Into<WritePipe>,
        child_watcher: ChildExitWatcher,
    ) -> Self {
        Self {
            backend: backend.into(),
            conout: conout.into(),
            conin: conin.into(),
            read_token: 0.into(),
            write_token: 0.into(),
            child_event_token: 0.into(),
            child_watcher,
        }
    }
}

impl EventedReadWrite for Pty {
    type Reader = ReadPipe;
    type Writer = WritePipe;

    #[inline]
    fn register(
        &mut self,
        poll: &mio::Poll,
        token: &mut dyn Iterator<Item = mio::Token>,
        interest: mio::Ready,
        poll_opts: mio::PollOpt,
    ) -> io::Result<()> {
        self.read_token = token.next().unwrap();
        self.write_token = token.next().unwrap();

        if interest.is_readable() {
            poll.register(&self.conout, self.read_token, mio::Ready::readable(), poll_opts)?
        } else {
            poll.register(&self.conout, self.read_token, mio::Ready::empty(), poll_opts)?
        }
        if interest.is_writable() {
            poll.register(&self.conin, self.write_token, mio::Ready::writable(), poll_opts)?
        } else {
            poll.register(&self.conin, self.write_token, mio::Ready::empty(), poll_opts)?
        }

        self.child_event_token = token.next().unwrap();
        poll.register(
            self.child_watcher.event_rx(),
            self.child_event_token,
            mio::Ready::readable(),
            poll_opts,
        )?;

        Ok(())
    }

    #[inline]
    fn reregister(
        &mut self,
        poll: &mio::Poll,
        interest: mio::Ready,
        poll_opts: mio::PollOpt,
    ) -> io::Result<()> {
        if interest.is_readable() {
            poll.reregister(&self.conout, self.read_token, mio::Ready::readable(), poll_opts)?;
        } else {
            poll.reregister(&self.conout, self.read_token, mio::Ready::empty(), poll_opts)?;
        }
        if interest.is_writable() {
            poll.reregister(&self.conin, self.write_token, mio::Ready::writable(), poll_opts)?;
        } else {
            poll.reregister(&self.conin, self.write_token, mio::Ready::empty(), poll_opts)?;
        }

        poll.reregister(
            self.child_watcher.event_rx(),
            self.child_event_token,
            mio::Ready::readable(),
            poll_opts,
        )?;

        Ok(())
    }

    #[inline]
    fn deregister(&mut self, poll: &mio::Poll) -> io::Result<()> {
        poll.deregister(&self.conout)?;
        poll.deregister(&self.conin)?;
        poll.deregister(self.child_watcher.event_rx())?;
        Ok(())
    }

    #[inline]
    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.conout
    }

    #[inline]
    fn read_token(&self) -> mio::Token {
        self.read_token
    }

    #[inline]
    fn writer(&mut self) -> &mut Self::Writer {
        &mut self.conin
    }

    #[inline]
    fn write_token(&self) -> mio::Token {
        self.write_token
    }
}

impl EventedPty for Pty {
    fn child_event_token(&self) -> mio::Token {
        self.child_event_token
    }

    fn next_child_event(&mut self) -> Option<ChildEvent> {
        match self.child_watcher.event_rx().try_recv() {
            Ok(ev) => Some(ev),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(ChildEvent::Exited),
        }
    }
}

impl OnResize for Pty {
    fn on_resize(&mut self, size: &SizeInfo) {
        self.backend.on_resize(size)
    }
}

fn cmdline<C>(config: &Config<C>) -> String {
    let default_shell = Program::Just("powershell".to_owned());
    let shell = config.shell.as_ref().unwrap_or(&default_shell);

    once(shell.program().as_ref())
        .chain(shell.args().iter().map(|a| a.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Converts the string slice into a Windows-standard representation for "W"-
/// suffixed function variants, which accept UTF-16 encoded string values.
pub fn win32_string<S: AsRef<OsStr> + ?Sized>(value: &S) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}
