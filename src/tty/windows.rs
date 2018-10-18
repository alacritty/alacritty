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

use std::io;
use std::fs::OpenOptions;
use std::os::raw::c_void;
use std::os::windows::io::{FromRawHandle, IntoRawHandle};
use std::os::windows::fs::OpenOptionsExt;
use std::env;
use std::cell::UnsafeCell;

use dunce::canonicalize;
use mio;
use mio::Evented;
use mio_named_pipes::NamedPipe;
use winapi::um::synchapi::WaitForSingleObject;
use winapi::um::winbase::{WAIT_OBJECT_0, FILE_FLAG_OVERLAPPED};
use winapi::shared::winerror::WAIT_TIMEOUT;
use winpty::{ConfigFlags, MouseMode, SpawnConfig, SpawnFlags, Winpty};
use winpty::Config as WinptyConfig;

use config::{Config, Shell};
use display::OnResize;
use cli::Options;
use tty::EventedReadWrite;
use term::SizeInfo;

/// Handle to the winpty agent process. Required so we know when it closes.
static mut HANDLE: *mut c_void = 0usize as *mut c_void;

/// How long the winpty agent should wait for any RPC request
/// This is a placeholder value until we see how often long responses happen
const AGENT_TIMEOUT: u32 = 10000;

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

pub struct Pty<'a, R: io::Read + Evented + Send, W: io::Write + Evented + Send> {
    // TODO: Provide methods for accessing this safely
    pub winpty: UnsafeCell<Winpty<'a>>,

    conout: R,
    conin: W,
    read_token: mio::Token,
    write_token: mio::Token,
}

pub fn new<'a>(
    config: &Config,
    options: &Options,
    size: &SizeInfo,
    _window_id: Option<usize>,
) -> Pty<'a, NamedPipe, NamedPipe> {
    // Create config
    let mut wconfig = WinptyConfig::new(ConfigFlags::empty()).unwrap();

    wconfig.set_initial_size(size.cols().0 as i32, size.lines().0 as i32);
    wconfig.set_mouse_mode(&MouseMode::Auto);
    wconfig.set_agent_timeout(AGENT_TIMEOUT);

    // Start agent
    let mut winpty = Winpty::open(&wconfig).unwrap();
    let (conin, conout) = (winpty.conin_name(), winpty.conout_name());

    // Get process commandline
    let default_shell = &Shell::new(env::var("COMSPEC").unwrap_or_else(|_| "cmd".into()));
    let shell = config.shell().unwrap_or(default_shell);
    let initial_command = options.command().unwrap_or(shell);
    let mut cmdline = initial_command.args().to_vec();
    cmdline.insert(0, initial_command.program().into());

    // Warning, here be borrow hell
    let cwd = options.working_dir.as_ref().map(|dir| canonicalize(dir).unwrap());
    let cwd = cwd.as_ref().map(|dir| dir.to_str().unwrap());

    // Spawn process
    let spawnconfig = SpawnConfig::new(
        SpawnFlags::AUTO_SHUTDOWN | SpawnFlags::EXIT_AFTER_SHUTDOWN,
        None, // appname
        Some(&cmdline.join(" ")),
        cwd,
        None, // Env
    ).unwrap();

    let default_opts = &mut OpenOptions::new();
    default_opts
        .share_mode(0)
        .custom_flags(FILE_FLAG_OVERLAPPED);

    let (conout_pipe, conin_pipe);
    unsafe {
        conout_pipe = NamedPipe::from_raw_handle(
            default_opts
                .clone()
                .read(true)
                .open(conout)
                .unwrap()
                .into_raw_handle(),
        );
        conin_pipe = NamedPipe::from_raw_handle(
            default_opts
                .clone()
                .write(true)
                .open(conin)
                .unwrap()
                .into_raw_handle(),
        );
    };

    if let Some(err) = conout_pipe.connect().err() {
        if err.kind() != io::ErrorKind::WouldBlock {
            panic!(err);
        }
    }
    assert!(conout_pipe.take_error().unwrap().is_none());

    if let Some(err) = conin_pipe.connect().err() {
        if err.kind() != io::ErrorKind::WouldBlock {
            panic!(err);
        }
    }
    assert!(conin_pipe.take_error().unwrap().is_none());

    winpty.spawn(&spawnconfig).unwrap();

    unsafe {
        HANDLE = winpty.raw_handle();
    }

    Pty {
        winpty: UnsafeCell::new(winpty),
        conout: conout_pipe,
        conin: conin_pipe,
        // Placeholder tokens that are overwritten
        read_token: 0.into(),
        write_token: 0.into(),
    }
}

impl<'a> EventedReadWrite for Pty<'a, NamedPipe, NamedPipe> {
    type Reader = NamedPipe;
    type Writer = NamedPipe;

    #[inline]
    fn register(
        &mut self,
        poll: &mio::Poll,
        token: &mut Iterator<Item = &usize>,
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
    fn reader(&mut self) -> &mut NamedPipe {
        &mut self.conout
    }

    #[inline]
    fn read_token(&self) -> mio::Token {
        self.read_token
    }

    #[inline]
    fn writer(&mut self) -> &mut NamedPipe {
        &mut self.conin
    }

    #[inline]
    fn write_token(&self) -> mio::Token {
        self.write_token
    }
}

impl<'a> OnResize for Winpty<'a> {
    fn on_resize(&mut self, sizeinfo: &SizeInfo) {
        if sizeinfo.cols().0 > 0 && sizeinfo.lines().0 > 0 {
            self.set_size(sizeinfo.cols().0, sizeinfo.lines().0)
                .unwrap_or_else(|_| info!("Unable to set winpty size, did it die?"));
        }
    }
}
