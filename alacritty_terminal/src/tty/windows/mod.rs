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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::TryRecvError;

use log::info;

use crate::config::Config;
use crate::event::OnResize;
use crate::term::SizeInfo;
use crate::tty::windows::child::ChildExitWatcher;
use crate::tty::{ChildEvent, EventedPty, EventedReadWrite};

mod child;
mod conpty;
mod winpty;

static IS_CONPTY: AtomicBool = AtomicBool::new(false);

pub fn is_conpty() -> bool {
    IS_CONPTY.load(Ordering::Relaxed)
}

enum PtyInner {
    Winpty(winpty::WinptyAgent),
    Conpty(conpty::Conpty),
}

pub struct Pty {
    inner: PtyInner,
    read_token: mio::Token,
    write_token: mio::Token,
    child_event_token: mio::Token,
    child_watcher: ChildExitWatcher,
}

impl Pty {
    pub fn resize_handle(&self) -> impl OnResize {
        match &self.inner {
            PtyInner::Winpty(w) => PtyResizeHandle::Winpty(w.resize_handle()),
            PtyInner::Conpty(c) => PtyResizeHandle::Conpty(c.resize_handle()),
        }
    }
}

enum PtyResizeHandle {
    Winpty(winpty::WinptyResizeHandle),
    Conpty(conpty::ConptyResizeHandle),
}

impl OnResize for PtyResizeHandle {
    fn on_resize(&mut self, size: &SizeInfo) {
        match self {
            PtyResizeHandle::Winpty(w) => w.on_resize(size),
            PtyResizeHandle::Conpty(c) => c.on_resize(size),
        }
    }
}

pub fn new<C>(config: &Config<C>, size: &SizeInfo, window_id: Option<usize>) -> Pty {
    if let Some(pty) = conpty::new(config, size, window_id) {
        info!("Using Conpty agent");
        IS_CONPTY.store(true, Ordering::Relaxed);
        pty
    } else {
        info!("Using Winpty agent");
        winpty::new(config, size, window_id)
    }
}

impl Read for Pty {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.inner {
            PtyInner::Winpty(ref mut w) => w.conout.read(buf),
            PtyInner::Conpty(ref mut c) => c.conout.read(buf),
        }
    }
}

impl Write for Pty {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.inner {
            PtyInner::Winpty(ref mut w) => w.conin.write(buf),
            PtyInner::Conpty(ref mut c) => c.conin.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.inner {
            PtyInner::Winpty(ref mut w) => w.conin.flush(),
            PtyInner::Conpty(ref mut c) => c.conin.flush(),
        }
    }
}

// Read portion of an interest
fn read_interest(interest: mio::Ready) -> mio::Ready {
    if interest.is_readable() {
        mio::Ready::readable()
    } else {
        mio::Ready::empty()
    }
}

// Write portion of an interest
fn write_interest(interest: mio::Ready) -> mio::Ready {
    if interest.is_writable() {
        mio::Ready::writable()
    } else {
        mio::Ready::empty()
    }
}

impl EventedReadWrite for Pty {
    type Reader = Pty;
    type Writer = Pty;

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

        let ri = read_interest(interest);
        let wi = write_interest(interest);

        match &self.inner {
            PtyInner::Winpty(w) => {
                poll.register(&w.conout, self.read_token, ri, poll_opts)?;
                poll.register(&w.conin, self.write_token, wi, poll_opts)?;
            }
            PtyInner::Conpty(c) => {
                poll.register(&c.conout, self.read_token, ri, poll_opts)?;
                poll.register(&c.conin, self.write_token, wi, poll_opts)?;
            }
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
        let ri = read_interest(interest);
        let wi = write_interest(interest);

        match &self.inner {
            PtyInner::Winpty(w) => {
                poll.reregister(&w.conout, self.read_token, ri, poll_opts)?;
                poll.reregister(&w.conin, self.write_token, wi, poll_opts)?;
            }
            PtyInner::Conpty(c) => {
                poll.reregister(&c.conout, self.read_token, ri, poll_opts)?;
                poll.reregister(&c.conin, self.write_token, wi, poll_opts)?;
            }
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
        match &self.inner {
            PtyInner::Winpty(w) => {
                poll.deregister(&w.conout)?;
                poll.deregister(&w.conin)?;
            }
            PtyInner::Conpty(c) => {
                poll.deregister(&c.conout)?;
                poll.deregister(&c.conin)?;
            }
        }

        poll.deregister(self.child_watcher.event_rx())?;
        Ok(())
    }

    #[inline]
    fn reader(&mut self) -> &mut Self::Reader {
        self
    }

    #[inline]
    fn read_token(&self) -> mio::Token {
        self.read_token
    }

    #[inline]
    fn writer(&mut self) -> &mut Self::Writer {
        self
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
