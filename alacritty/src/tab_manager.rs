use std::os::unix::io::AsRawFd;

use libc::winsize;
use objc::sel_impl;
use std::io;


use std::{
    ffi::OsStr,
    fs::File,
    io::Read,
    process::{Command, Stdio},
    thread,
};

use anyhow::Result;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use std::io::Write;

use std::ops::{Deref, Index, IndexMut, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo};



use std::marker::PhantomData;

use std::hash::{BuildHasher, Hash};

use miniserde::ser::{Fragment, Map, Seq};

use std::borrow::Cow;

use miniserde::{json, Deserialize, Serialize};

pub const DEFAULT_SHELL: &str = "/bin/zsh";

use log::{debug, error, info, warn};

use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;


use alacritty_terminal::term::SizeInfo;

use crate::child_pty::ChildPty;
use crate::child_pty::PtyUpdate;

use crate::event::EventProxy;
use thiserror::Error;

use crate::config::Config;

const DELAY_DURATION: Duration = Duration::from_millis(400);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TabManagerUpdate {
    tab_idx: usize,
    data: PtyUpdate,
}

macro_rules! seltab_cl {
    ($seltab:expr, $terminal:ident,  { $($b:tt)* } ) => {
        let tab = $seltab.unwrap();
        let terminal_mutex =  tab.terminal.clone();
        let mut terminal_guard = terminal_mutex.lock();
        let mut $terminal = &mut *terminal_guard;

        $($b)*

        drop(terminal_guard);


    };
}

#[derive(Debug)]
pub enum Msg {
    /// Data that should be written to the PTY.
    Input(Vec<u8>),

    /// Instruction to resize the PTY.
    Resize(SizeInfo),
}

pub struct TabManager {
    pub to_exit: RwLock<bool>,
    pub selected_tab: RwLock<Option<usize>>,
    pub tabs: RwLock<Vec<Tab>>,
    pub size: RwLock<Option<SizeInfo>>,
    pub event_proxy: crate::event::EventProxy,
    pub config: Config,
    pub last_update: std::time::Instant,
}

impl TabManager {
    pub fn set_to_exit(&self) {
        let mut to_exit_guard = self.to_exit.write().unwrap();
        let mut to_exit_mut = &mut *to_exit_guard;
        *to_exit_mut = true;
        drop(to_exit_guard);
    }
    pub fn new(event_proxy: crate::event::EventProxy, config: Config) -> TabManager {
        let mut tm = Self {
            to_exit: RwLock::new(false),
            selected_tab: RwLock::new(None),
            tabs: RwLock::new(Vec::new()),
            size: RwLock::new(None),
            event_proxy,
            config,
            last_update: Instant::now(),
        };

        return tm;
    }

    pub fn resize(&self, sz: SizeInfo) {
        let mut size_guard = self.size.write().unwrap();
        let mut size  = &mut *size_guard;
        *size = Some(sz);
         drop(size_guard);

        let tab_r = &*self.tabs.read().unwrap();

        for tab in tab_r.into_iter() {
            let terminal_mutex = tab.terminal.clone();
            let mut terminal_guard = terminal_mutex.lock();
            let mut terminal = &mut *terminal_guard;
            let term_sz = sz.clone();
            terminal.resize(term_sz);
            drop(terminal_guard);

            let pty_mutex = tab.pty.clone();
            let mut pty_guard = pty_mutex.lock();
            let mut pty = &mut *pty_guard;
            let pty_sz = sz.clone();
            pty.on_resize(&pty_sz);
            drop(pty_guard);
        }
    }

    pub fn set_size(&self, size: SizeInfo) {
        let mut size_guard = self.size.write().unwrap();
        let mut size_mut  = &mut *size_guard;
        let size_clone = size.clone();
        *size_mut = Some(size_clone);
         drop(size_guard);
    }

    #[inline]
    pub fn num_tabs(&self) -> usize {
        let tabs = &*self.tabs.read().unwrap();
        tabs.len()
    }
    pub fn get_next_tab(&mut self) -> usize {
        match self.selected_tab_idx() {
            Some(idx) => {
                if idx + 1 >= self.num_tabs()  {
                    0
                } else {
                    idx + 1
                }
            },
            None => 0,
        }
    }

    pub fn new_tab(&self) -> Result<usize> {
        let tab_idx = match self.selected_tab_idx() {
            Some(idx) => idx + 1,
            None => 0,
        };
        info!("Creating new tab {}\n", tab_idx);
        info!("Default shell {}\n", DEFAULT_SHELL);
        let szinfo = (*self.size.read().unwrap()).unwrap();
        let new_tab = Tab::new(
            DEFAULT_SHELL,
            szinfo.clone(),
            self.config.clone(),
            self.event_proxy.clone(),
            self,
        );

        let pty_arc = new_tab.pty.clone();
        let mut pty_guard = pty_arc.lock();
        let mut unlocked_pty = &mut *pty_guard;
        let raw_fd: std::os::unix::io::RawFd = unlocked_pty.file.as_raw_fd();
        let mut pty_output_file = unlocked_pty.file.try_clone().unwrap();
        drop(pty_guard);

        let terminal_arc = new_tab.terminal.clone();
        
        let mut tabs_guard = self.tabs.write().unwrap();
        let mut tabs = &mut *tabs_guard;
        tabs.push(new_tab);
        drop(tabs_guard);

        if self.num_tabs() == 1 {
            self.set_selected_tab(0);
        }

        info!("Inserted and selected new tab {}\n", tab_idx);

        let event_proxy = self.event_proxy.clone();
        thread::spawn(move || {
            let mut processor = alacritty_terminal::ansi::Processor::new();
            loop {
                let mut buffer: [u8; crate::child_pty::PTY_BUFFER_SIZE] =
                    [0; crate::child_pty::PTY_BUFFER_SIZE];

                match pty_output_file.read(&mut buffer) {
                    Ok(rlen) => {
                        if rlen > 0 {
                            let mut terminal_guard = terminal_arc.lock();
                            let mut terminal = &mut *terminal_guard;
                            let mut pty_guard = pty_arc.lock();
                            let mut unlocked_pty = &mut *pty_guard;
                            
                            // event_proxy.send_event(crate::event::Event::TerminalEvent(
                            //     alacritty_terminal::event::Event::Wakeup,
                            // ));
                            buffer.into_iter().for_each(|byte| {
                                processor.advance(terminal, *byte, &mut unlocked_pty)
                            });

                            // terminal.dirty = true;
                            
                            drop(pty_guard);
                            drop(terminal_guard);
                        }

                        if rlen == 0 {
                            // Close this tty
                            event_proxy.send_event(crate::event::Event::TerminalEvent(
                                alacritty_terminal::event::Event::Close(tab_idx),
                            ));
                            break; // break out of loop
                        }
                    },
                    Err(e) => {
                        error!("Error {} reading bytes from tty", e);
                    },
                }
            }
        });

        Ok(tab_idx)
    }

    pub fn set_selected_tab(&self, idx: usize) {
        let mut wg = self.selected_tab.write().unwrap();
        let mut sel_tab = &mut *wg;
        *sel_tab = Some(idx);
        drop(wg);
    }

    pub fn remove_selected_tab(&self) {
        match self.selected_tab_idx() {
            Some(idx) => {
                let tabs_guard = self.tabs.write().unwrap().remove(idx);
            },
            None => {},
        };

        if self.num_tabs() == 0 {
            match self.new_tab() {
                Ok(idx) => {
                    self.set_selected_tab(0);
                },
                Err(e) => {
                    error!("Attempted to remove a tab when no tabs exist");
                },
            }
        } else {
            let next_idx = self.next_tab_idx().unwrap();
            if next_idx >= self.num_tabs() {
                self.set_selected_tab(self.num_tabs() - 1);
            } else {
                self.set_selected_tab(next_idx);
            }
        }
    }

    pub fn get_selected_tab_pty(&self) -> Arc<FairMutex<ChildPty>> {
        self.selected_tab_arc().pty.clone()
    }

    pub fn get_selected_tab_terminal(&self) -> Arc<FairMutex<Term<EventProxy>>> {
        self.selected_tab_arc().terminal.clone()
    }

    fn selected_tab_arc(&self) -> Arc<Tab> {
        match self.selected_tab_idx() {
            Some(sel_idx) => {
                let tabs_guard = self.tabs.read().unwrap();
                let tabs = & *tabs_guard;
                let tab = tabs.get(sel_idx).unwrap();
                let tab_clone = tab.clone();
                Arc::new(tab_clone)
            },
            None => {
                if self.num_tabs() == 0 {
                    match self.new_tab() {
                        Ok(idx) => {
                            info!("Created new tab {}", idx);
                        },
                        Err(e) => {
                            error!("Error creating new tab");
                        },
                    }
                }
                self.set_selected_tab(0);
                let tabs_guard = self.tabs.read().unwrap();
                let tabs = & *tabs_guard;
                let tab = tabs.get(0).unwrap().clone(); 
                Arc::new(tab)
            },
        }
    }

    pub fn selected_tab_mut(&mut self) -> &mut Tab {
        match self.selected_tab_idx() {
            Some(sel_idx) => {
                let mut tabs = self.tabs.get_mut().unwrap();
                tabs.get_mut(sel_idx).unwrap()
            },
            None => {
                if self.num_tabs() == 0 {
                    match self.new_tab() {
                        Ok(idx) => {
                            info!("Created new tab {}", idx);
                        },
                        Err(e) => {
                            error!("Error creating new tab");
                        },
                    }
                }
                self.set_selected_tab(0);
                let mut tabs = self.tabs.get_mut().unwrap();
                tabs.get_mut(0).unwrap()
            },
        }
    }

    pub fn select_tab(& self, idx: usize) -> Option<usize> {
        self.set_selected_tab(idx);
        self.selected_tab_idx()
    }

    #[inline]
    pub fn selected_tab_idx(&self) -> Option<usize> {
        *self.selected_tab.read().unwrap()
    }

    /// Get index of next oldest tab.
    pub fn next_tab_idx(&self) -> Option<usize> {
        match self.selected_tab_idx() {
            Some(idx) => {
                if self.num_tabs() == 0 {
                    None
                } else if idx + 1 >= self.num_tabs() {
                    Some(0)
                } else {
                    Some(idx + 1)
                }
            },
            None => None,
        }
    }

    /// Get index of next older tab.
    pub fn prev_tab_idx(&self) -> Option<usize> {
        match self.selected_tab_idx() {
            Some(idx) => {
                if idx == 0 {
                    if self.num_tabs() > 1 {
                        Some(self.num_tabs() - 1)
                    } else {
                        Some(0)
                    }
                } else {
                    Some(idx - 1)
                }
            },
            None => None,
        }
    }

    /// Get index of youngest tab.
    pub fn first_tab_idx(&self) -> Option<usize> {
        // Next here will just iterate to the first value
        Some(0)
    }

    /// Get index of oldest tab.
    pub fn last_tab_idx(&self) -> Option<usize> {
        // Next back will iterate back around to the last value
        Some(self.num_tabs())
    }

    /// Receive stdin for the active `Window`.
    pub fn receive_stdin(& self, data: &[u8]) -> Result<(), TabError> {
        let sel_idx_option = *self.selected_tab.read().unwrap();
        let sel_idx = sel_idx_option.unwrap();

        let tab_rw = self.tabs.read();
        let tabs = tab_rw.unwrap();

        let tab = tabs.get(sel_idx).unwrap();
        Ok(tab.receive_stdin(data).unwrap())
        // Ok(self.selected_tab_mut().receive_stdin(data)?)
    }
}

pub fn stringify(err: &dyn std::fmt::Display) -> String {
    format!("error code: {}", err.to_string())
}
// pub fn stringify(err: &dyn std::error::Error) -> String { format!("error code: {}",
// err.to_string()) }

#[derive(Error, Debug)]
pub enum TabError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("no selected tab")]
    NoSelectedTab,
    #[error("attempted to select an invalid tab")]
    TabLost,
}

#[derive(Error, Debug)]
pub enum TabWriteError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Unable to write for some other reason")]
    UnableToWriteOtherReason,
}

#[derive(Clone)]
pub struct Tab {
    pub pty: Arc<FairMutex<ChildPty>>,
    pub terminal: Arc<FairMutex<Term<EventProxy>>>,
}

impl Tab {
    pub fn new(
        command: &str,
        size: SizeInfo,
        config: Config,
        event_proxy: crate::event::EventProxy,
        tab_manager: & TabManager,
    ) -> Tab {
        let terminal = Term::new(&config, size, event_proxy.clone());
        let terminal = Arc::new(FairMutex::new(terminal));

        let new_winsize = winsize {
            ws_row: size.screen_lines().0 as u16,
            ws_col: size.cols().0 as u16,
            ws_xpixel: size.width() as libc::c_ushort,
            ws_ypixel: size.height() as libc::c_ushort,
        };

        let args: [&str; 0] = [];
        let pty = Arc::new(FairMutex::new(ChildPty::new(command, &args, new_winsize).unwrap()));

        Tab { pty, terminal }
    }

    pub fn receive_stdin(&self, data: &[u8]) -> Result<(), io::Error> {
        let tab_terminal = self.terminal.clone();
        let mut terminal_guard = tab_terminal.lock();
        let terminal = &mut *terminal_guard;
        // terminal.dirty = true;
        drop(terminal_guard);

        let mut pty_guard = self.pty.lock();
        let mut unlocked_pty = &mut *pty_guard;
        unlocked_pty.write(data)?;
        drop(pty_guard);
        Ok(())
    }
    
}
