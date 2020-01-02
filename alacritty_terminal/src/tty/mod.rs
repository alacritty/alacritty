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
//
//! tty related functionality
use mio;
use std::{env, io};

use terminfo::Database;

use crate::config::Config;

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
pub use self::unix::*;

#[cfg(windows)]
pub mod windows;
#[cfg(windows)]
pub use self::windows::*;

/// This trait defines the behaviour needed to read and/or write to a stream.
/// It defines an abstraction over mio's interface in order to allow either one
/// read/write object or a separate read and write object.
pub trait EventedReadWrite {
    type Reader: io::Read;
    type Writer: io::Write;

    fn register(
        &mut self,
        _: &mio::Poll,
        _: &mut dyn Iterator<Item = mio::Token>,
        _: mio::Ready,
        _: mio::PollOpt,
    ) -> io::Result<()>;
    fn reregister(&mut self, _: &mio::Poll, _: mio::Ready, _: mio::PollOpt) -> io::Result<()>;
    fn deregister(&mut self, _: &mio::Poll) -> io::Result<()>;

    fn reader(&mut self) -> &mut Self::Reader;
    fn read_token(&self) -> mio::Token;
    fn writer(&mut self) -> &mut Self::Writer;
    fn write_token(&self) -> mio::Token;
}

/// Events concerning TTY child processes
#[derive(Debug, PartialEq)]
pub enum ChildEvent {
    /// Indicates the child has exited
    Exited,
}

/// A pseudoterminal (or PTY)
///
/// This is a refinement of EventedReadWrite that also provides a channel through which we can be
/// notified if the PTY child process does something we care about (other than writing to the TTY).
/// In particular, this allows for race-free child exit notification on UNIX (cf. `SIGCHLD`).
pub trait EventedPty: EventedReadWrite {
    fn child_event_token(&self) -> mio::Token;

    /// Tries to retrieve an event
    ///
    /// Returns `Some(event)` on success, or `None` if there are no events to retrieve.
    fn next_child_event(&mut self) -> Option<ChildEvent>;
}

// Setup environment variables
pub fn setup_env<C>(config: &Config<C>) {
    // Default to 'alacritty' terminfo if it is available, otherwise
    // default to 'xterm-256color'. May be overridden by user's config
    // below.
    env::set_var(
        "TERM",
        if Database::from_name("alacritty").is_ok() { "alacritty" } else { "xterm-256color" },
    );

    // Advertise 24-bit color support
    env::set_var("COLORTERM", "truecolor");

    // Prevent child processes from inheriting startup notification env
    env::remove_var("DESKTOP_STARTUP_ID");

    // Set env vars from config
    for (key, value) in config.env.iter() {
        env::set_var(key, value);
    }
}
