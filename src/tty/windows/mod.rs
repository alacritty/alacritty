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

use std::io::{self, Read, Write};
use std::os::raw::c_void;

use mio::{self, Evented, Poll, PollOpt, Ready, Token};
use mio_anonymous_pipes::{EventedAnonRead, EventedAnonWrite};
use mio_named_pipes::NamedPipe;

use winapi::um::synchapi::WaitForSingleObject;
use winapi::um::winbase::WAIT_OBJECT_0;
use winapi::shared::winerror::WAIT_TIMEOUT;

use crate::config::Config;
use crate::display::OnResize;
use crate::cli::Options;
use crate::tty::EventedReadWrite;
use crate::term::SizeInfo;

mod conpty;
mod winpty;

/// Handle to the winpty agent or conpty process. Required so we know when it closes.
static mut HANDLE: *mut c_void = 0usize as *mut c_void;

pub fn process_should_exit() -> bool {
    unsafe {
        match WaitForSingleObject(HANDLE, 0) {
            // Process has exited
            WAIT_OBJECT_0 => {
                info!("wait_object_0");
                true
            }
            // Reached timeout of 0, process has not exited
            WAIT_TIMEOUT => false,
            // Error checking process, winpty gave us a bad agent handle?
            _ => {
                info!("Bad exit: {}", ::std::io::Error::last_os_error());
                true
            }
        }
    }
}

#[derive(Clone)]
pub enum PtyHandle<'a> {
    Winpty(winpty::WinptyHandle<'a>),
    Conpty(conpty::ConptyHandle)
}

pub struct Pty<'a> {
    handle: PtyHandle<'a>,
    // TODO: It's on the roadmap for the Conpty API to support Overlapped I/O.
    // See https://github.com/Microsoft/console/issues/262
    // When support for that lands then it should be possible to use
    // NamedPipe for the conout and conin handles
    conout: EventedReadablePipe,
    conin: EventedWritablePipe,
    read_token: mio::Token,
    write_token: mio::Token,
}

impl<'a> Pty<'a> {
    pub fn resize_handle(&self) -> impl OnResize + 'a {
        self.handle.clone()
    }
}

pub fn new<'a>(
    config: &Config,
    options: &Options,
    size: &SizeInfo,
    window_id: Option<usize>,
) -> Pty<'a> {
    if let Some(pty) = conpty::new(config, options, size, window_id) {
        info!("Using Conpty agent.");
        pty
    } else {
        info!("Using Winpty agent.");
        winpty::new(config, options, size, window_id)
    }
}

// TODO: The ConPTY API curently must use synchronous pipes as the input
// and output handles. This has led to the need to support two different
// types of pipe.
//
// When https://github.com/Microsoft/console/issues/262 lands then the
// Anonymous variant of this enum can be removed from the codebase and
// everything can just use NamedPipe.
pub enum EventedReadablePipe {
    Anonymous(EventedAnonRead),
    Named(NamedPipe)
}

pub enum EventedWritablePipe {
    Anonymous(EventedAnonWrite),
    Named(NamedPipe)
}

impl Evented for EventedReadablePipe {
    fn register(&self,
                poll: &Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt) -> io::Result<()> {
        match self {
            EventedReadablePipe::Anonymous(p) => p.register(poll, token, interest, opts),
            EventedReadablePipe::Named(p) => p.register(poll, token, interest, opts)
        }
    }

    fn reregister(&self,
                poll: &Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt) -> io::Result<()> {
        match self {
            EventedReadablePipe::Anonymous(p) => p.reregister(poll, token, interest, opts),
            EventedReadablePipe::Named(p) => p.reregister(poll, token, interest, opts)
        }
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        match self {
            EventedReadablePipe::Anonymous(p) => p.deregister(poll),
            EventedReadablePipe::Named(p) => p.deregister(poll)
        }
    }
}

impl Read for EventedReadablePipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            EventedReadablePipe::Anonymous(p) => p.read(buf),
            EventedReadablePipe::Named(p) => p.read(buf)
        }
    }
}

impl Evented for EventedWritablePipe {
    fn register(&self,
                poll: &Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt) -> io::Result<()> {
        match self {
            EventedWritablePipe::Anonymous(p) => p.register(poll, token, interest, opts),
            EventedWritablePipe::Named(p) => p.register(poll, token, interest, opts)
        }
    }

    fn reregister(&self,
                poll: &Poll,
                token: Token,
                interest: Ready,
                opts: PollOpt) -> io::Result<()> {
        match self {
            EventedWritablePipe::Anonymous(p) => p.reregister(poll, token, interest, opts),
            EventedWritablePipe::Named(p) => p.reregister(poll, token, interest, opts)
        }
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        match self {
            EventedWritablePipe::Anonymous(p) => p.deregister(poll),
            EventedWritablePipe::Named(p) => p.deregister(poll)
        }
    }
}

impl Write for EventedWritablePipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            EventedWritablePipe::Anonymous(p) => p.write(buf),
            EventedWritablePipe::Named(p) => p.write(buf)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            EventedWritablePipe::Anonymous(p) => p.flush(),
            EventedWritablePipe::Named(p) => p.flush()
        }
    }
}

impl<'a> OnResize for PtyHandle<'a> {
    fn on_resize(&mut self, sizeinfo: &SizeInfo) {
        match self {
            PtyHandle::Winpty(w) => w.winpty_mut().on_resize(sizeinfo),
            PtyHandle::Conpty(c) => {
                let mut handle = c.clone();
                handle.on_resize(sizeinfo)
            }
        }
    }
}

impl<'a> EventedReadWrite for Pty<'a> {
    type Reader = EventedReadablePipe;
    type Writer = EventedWritablePipe;

    #[inline]
    fn register(
        &mut self,
        poll: &mio::Poll,
        token: &mut dyn Iterator<Item = &usize>,
        interest: mio::Ready,
        poll_opts: mio::PollOpt,
    ) -> io::Result<()> {
        self.read_token = (*token.next().unwrap()).into();
        self.write_token = (*token.next().unwrap()).into();

        if interest.is_readable() {
            poll.register(
                &self.conout,
                self.read_token,
                mio::Ready::readable(),
                poll_opts,
            )?
        } else {
            poll.register(
                &self.conout,
                self.read_token,
                mio::Ready::empty(),
                poll_opts,
            )?
        }
        if interest.is_writable() {
            poll.register(
                &self.conin,
                self.write_token,
                mio::Ready::writable(),
                poll_opts,
            )?
        } else {
            poll.register(
                &self.conin,
                self.write_token,
                mio::Ready::empty(),
                poll_opts,
            )?
        }
        Ok(())
    }

    #[inline]
    fn reregister(&mut self, poll: &mio::Poll, interest: mio::Ready, poll_opts: mio::PollOpt) -> io::Result<()> {
        if interest.is_readable() {
            poll.reregister(
                &self.conout,
                self.read_token,
                mio::Ready::readable(),
                poll_opts,
            )?;
        } else {
            poll.reregister(
                &self.conout,
                self.read_token,
                mio::Ready::empty(),
                poll_opts,
            )?;
        }
        if interest.is_writable() {
            poll.reregister(
                &self.conin,
                self.write_token,
                mio::Ready::writable(),
                poll_opts,
            )?;
        } else {
            poll.reregister(
                &self.conin,
                self.write_token,
                mio::Ready::empty(),
                poll_opts,
            )?;
        }
        Ok(())
    }

    #[inline]
    fn deregister(&mut self, poll: &mio::Poll) -> io::Result<()> {
        poll.deregister(&self.conout)?;
        poll.deregister(&self.conin)?;
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
