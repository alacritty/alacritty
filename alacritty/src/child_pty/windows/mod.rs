use std::io::prelude::*;
use std::ffi::OsStr;
use std::io;
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::sync::mpsc::TryRecvError;

use alacritty_terminal::config::{Config, Program};
use alacritty_terminal::term::SizeInfo;
// use crate::child_pty::windows::child::ChildExitWatcher;

mod child;
mod conpty;

use conpty::Conpty as Backend;
use mio_anonymous_pipes::{EventedAnonRead as ReadPipe, EventedAnonWrite as WritePipe};


pub fn new<C>(config: &Config<C>, size: &SizeInfo, _window_id: Option<usize>) -> Pty {
    conpty::new(config, size).expect("Failed to create ConPTY backend")
}


pub struct Pty {
    // XXX: Backend is required to be the first field, to ensure correct drop order. Dropping
    // `conout` before `backend` will cause a deadlock (with Conpty).
    pub backend: Backend,
    pub fout: ReadPipe,
    pub fin: WritePipe,
    // pub read_token: mio::Token,
    // pub write_token: mio::Token,
    // pub child_event_token: mio::Token,
    // child_watcher: ChildExitWatcher,
}

impl io::Read for Pty {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let nbytes = self.fout.read(buf)?;
        Ok(nbytes)
    }
}

impl std::io::Write for Pty {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.fin.write(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::result::Result<(), std::io::Error> {
        self.fin.flush()?;
        Ok(())
    }
}

impl Pty {
    fn new(
        backend: impl Into<Backend>,
        conout: impl Into<ReadPipe>,
        conin: impl Into<WritePipe>,
        // child_watcher: ChildExitWatcher,
    ) -> Self {
        Self {
            backend: backend.into(),
            fout: conout.into(),
            fin: conin.into(),
            // read_token: 0.into(),
            // write_token: 0.into(),
            // child_event_token: 0.into(),
            // child_watcher,
        }
    }

    fn on_resize(&mut self, size: &SizeInfo) {
        self.backend.on_resize(size)
    }
}

// impl EventedReadWrite for Pty {
//     type Reader = ReadPipe;
//     type Writer = WritePipe;

//     #[inline]
//     fn register(
//         &mut self,
//         poll: &mio::Poll,
//         token: &mut dyn Iterator<Item = mio::Token>,
//         interest: mio::Ready,
//         poll_opts: mio::PollOpt,
//     ) -> io::Result<()> {
//         self.read_token = token.next().unwrap();
//         self.write_token = token.next().unwrap();

//         if interest.is_readable() {
//             poll.register(&self.conout, self.read_token, mio::Ready::readable(), poll_opts)?
//         } else {
//             poll.register(&self.conout, self.read_token, mio::Ready::empty(), poll_opts)?
//         }
//         if interest.is_writable() {
//             poll.register(&self.conin, self.write_token, mio::Ready::writable(), poll_opts)?
//         } else {
//             poll.register(&self.conin, self.write_token, mio::Ready::empty(), poll_opts)?
//         }

//         self.child_event_token = token.next().unwrap();
//         poll.register(
//             self.child_watcher.event_rx(),
//             self.child_event_token,
//             mio::Ready::readable(),
//             poll_opts,
//         )?;

//         Ok(())
//     }

//     #[inline]
//     fn reregister(
//         &mut self,
//         poll: &mio::Poll,
//         interest: mio::Ready,
//         poll_opts: mio::PollOpt,
//     ) -> io::Result<()> {
//         if interest.is_readable() {
//             poll.reregister(&self.conout, self.read_token, mio::Ready::readable(), poll_opts)?;
//         } else {
//             poll.reregister(&self.conout, self.read_token, mio::Ready::empty(), poll_opts)?;
//         }
//         if interest.is_writable() {
//             poll.reregister(&self.conin, self.write_token, mio::Ready::writable(), poll_opts)?;
//         } else {
//             poll.reregister(&self.conin, self.write_token, mio::Ready::empty(), poll_opts)?;
//         }

//         poll.reregister(
//             self.child_watcher.event_rx(),
//             self.child_event_token,
//             mio::Ready::readable(),
//             poll_opts,
//         )?;

//         Ok(())
//     }

//     #[inline]
//     fn deregister(&mut self, poll: &mio::Poll) -> io::Result<()> {
//         poll.deregister(&self.conout)?;
//         poll.deregister(&self.conin)?;
//         poll.deregister(self.child_watcher.event_rx())?;
//         Ok(())
//     }

//     #[inline]
//     fn reader(&mut self) -> &mut Self::Reader {
//         &mut self.conout
//     }

//     #[inline]
//     fn read_token(&self) -> mio::Token {
//         self.read_token
//     }

//     #[inline]
//     fn writer(&mut self) -> &mut Self::Writer {
//         &mut self.conin
//     }

//     #[inline]
//     fn write_token(&self) -> mio::Token {
//         self.write_token
//     }
// }

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
