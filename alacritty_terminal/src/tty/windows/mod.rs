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
use mio::Evented;

use crate::config::Config;
use crate::event::OnResize;
use crate::term::SizeInfo;
use crate::tty::windows::child::ChildExitWatcher;
use crate::tty::{ChildEvent, EventedPty, EventedReadWrite};

mod child;
mod conpty;
mod dynamic;
mod winpty;

static IS_CONPTY: AtomicBool = AtomicBool::new(false);

pub fn is_conpty() -> bool {
    IS_CONPTY.load(Ordering::Relaxed)
}

pub struct Pty<T> {
    inner: T,
    read_token: mio::Token,
    write_token: mio::Token,
    child_event_token: mio::Token,
    child_watcher: ChildExitWatcher,
}

impl<T: PtyImpl> Pty<T> {
    fn new(inner: T, child_watcher: ChildExitWatcher) -> Self {
        Self {
            inner,
            read_token: 0.into(),
            write_token: 0.into(),
            child_event_token: 0.into(),
            child_watcher,
        }
    }

    pub fn resize_handle(&self) -> impl OnResize {
        self.inner.resize_handle()
    }
}

pub trait EventedRead: Read + Evented {}
impl<T: Read + Evented> EventedRead for T {}

pub trait EventedWrite: Write + Evented {}
impl<T: Write + Evented> EventedWrite for T {}

pub trait PtyImpl {
    type ResizeHandle: OnResize;
    type Conout: EventedRead + ?Sized;
    type Conin: EventedWrite + ?Sized;

    fn resize_handle(&self) -> Self::ResizeHandle;
    fn conout(&self) -> &Self::Conout;
    fn conout_mut(&mut self) -> &mut Self::Conout;
    fn conin(&self) -> &Self::Conin;
    fn conin_mut(&mut self) -> &mut Self::Conin;
}

pub fn new<C>(config: &Config<C>, size: &SizeInfo, window_id: Option<usize>) -> Pty<impl PtyImpl> {
    use dynamic::IntoDynamicPty;

    if let Some(pty) = conpty::new(config, size, window_id) {
        info!("Using Conpty agent");
        IS_CONPTY.store(true, Ordering::Relaxed);
        pty.into_dynamic_pty()
    } else {
        info!("Using Winpty agent");
        winpty::new(config, size, window_id).into_dynamic_pty()
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

impl<T: PtyImpl> EventedReadWrite for Pty<T> {
    type Reader = <T as PtyImpl>::Conout;
    type Writer = <T as PtyImpl>::Conin;

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

        poll.register(self.inner.conout(), self.read_token, read_interest(interest), poll_opts)?;
        poll.register(self.inner.conin(), self.write_token, write_interest(interest), poll_opts)?;

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

        poll.reregister(self.inner.conout(), self.read_token, ri, poll_opts)?;
        poll.reregister(self.inner.conin(), self.write_token, wi, poll_opts)?;

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
        poll.deregister(self.inner.conout())?;
        poll.deregister(self.inner.conin())?;
        poll.deregister(self.child_watcher.event_rx())?;
        Ok(())
    }

    #[inline]
    fn reader(&mut self) -> &mut Self::Reader {
        self.inner.conout_mut()
    }

    #[inline]
    fn read_token(&self) -> mio::Token {
        self.read_token
    }

    #[inline]
    fn writer(&mut self) -> &mut Self::Writer {
        self.inner.conin_mut()
    }

    #[inline]
    fn write_token(&self) -> mio::Token {
        self.write_token
    }
}

impl<T: PtyImpl> EventedPty for Pty<T> {
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
