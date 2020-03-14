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

use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{FromRawHandle, IntoRawHandle};
use std::u16;

use log::info;
use mio_named_pipes::NamedPipe;
use winapi::um::winbase::FILE_FLAG_OVERLAPPED;
use winpty::{Config as WinptyConfig, ConfigFlags, MouseMode, SpawnConfig, SpawnFlags, Winpty};

use crate::config::Config;
use crate::event::OnResize;
use crate::term::SizeInfo;
use crate::tty::windows::child::ChildExitWatcher;
use crate::tty::windows::{cmdline, Pty};

pub use winpty::Winpty as Agent;

pub fn new<C>(config: &Config<C>, size: &SizeInfo, _window_id: Option<usize>) -> Pty {
    // Create config
    let mut wconfig = WinptyConfig::new(ConfigFlags::empty()).unwrap();

    wconfig.set_initial_size(size.cols().0 as i32, size.lines().0 as i32);
    wconfig.set_mouse_mode(&MouseMode::Auto);

    // Start agent
    let mut agent = Winpty::open(&wconfig).unwrap();
    let (conin, conout) = (agent.conin_name(), agent.conout_name());

    let cmdline = cmdline(&config);

    // Spawn process
    let spawnconfig = SpawnConfig::new(
        SpawnFlags::AUTO_SHUTDOWN | SpawnFlags::EXIT_AFTER_SHUTDOWN,
        None, // appname
        Some(&cmdline),
        config.working_directory.as_ref().map(|p| p.as_path()),
        None, // Env
    )
    .unwrap();

    let default_opts = &mut OpenOptions::new();
    default_opts.share_mode(0).custom_flags(FILE_FLAG_OVERLAPPED);

    let (conout_pipe, conin_pipe);
    unsafe {
        conout_pipe = NamedPipe::from_raw_handle(
            default_opts.clone().read(true).open(conout).unwrap().into_raw_handle(),
        );
        conin_pipe = NamedPipe::from_raw_handle(
            default_opts.clone().write(true).open(conin).unwrap().into_raw_handle(),
        );
    };

    agent.spawn(&spawnconfig).unwrap();

    let child_watcher = ChildExitWatcher::new(agent.raw_handle()).unwrap();

    Pty {
        backend: super::PtyBackend::Winpty(agent),
        conout: super::EventedReadablePipe::Named(conout_pipe),
        conin: super::EventedWritablePipe::Named(conin_pipe),
        read_token: 0.into(),
        write_token: 0.into(),
        child_event_token: 0.into(),
        child_watcher,
    }
}

impl OnResize for Agent {
    fn on_resize(&mut self, sizeinfo: &SizeInfo) {
        let (cols, lines) = (sizeinfo.cols().0, sizeinfo.lines().0);
        if cols > 0 && cols <= u16::MAX as usize && lines > 0 && lines <= u16::MAX as usize {
            self.set_size(cols as u16, lines as u16)
                .unwrap_or_else(|_| info!("Unable to set WinPTY size, did it die?"));
        }
    }
}
