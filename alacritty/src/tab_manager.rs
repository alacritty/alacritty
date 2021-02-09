use libc::winsize;

use std::io;

use std::{
    io::Read,
    thread,
};

use anyhow::Result;
use std::sync::{Arc, RwLock};
use std::time::Instant;

pub const DEFAULT_SHELL: &str = "/bin/zsh";

use log::{error, info};

use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;


use alacritty_terminal::term::SizeInfo;

use crate::child_pty::ChildPty;

use alacritty_terminal::event::EventListener;

use crate::config::Config;

pub struct TabManager<T> {
    pub to_exit: RwLock<bool>,
    pub selected_tab: RwLock<Option<usize>>,
    pub tabs: RwLock<Vec<Tab<T>>>,
    pub size: RwLock<Option<SizeInfo>>,
    pub event_proxy: T,
    pub config: Config,
    pub last_update: std::time::Instant,
}

impl<T: Clone + EventListener + Send + 'static> TabManager<T> {
    pub fn new(event_proxy: T, config: Config) -> TabManager<T> {
        Self {
            to_exit: RwLock::new(false),
            selected_tab: RwLock::new(None),
            tabs: RwLock::new(Vec::new()),
            size: RwLock::new(None),
            event_proxy,
            config,
            last_update: Instant::now(),
        }
    }

    pub fn resize(&self, sz: SizeInfo) {
        *(&mut *(self.size.write().unwrap())) = Some(sz);

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
        *(&mut *size_guard) = Some(size.clone());
         drop(size_guard);
    }

    #[inline]
    pub fn num_tabs(&self) -> usize {
        let tabs = &*self.tabs.read().unwrap();
        tabs.len()
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
        );

        let pty_arc = new_tab.pty.clone();
        let mut pty_guard = pty_arc.lock();
        let unlocked_pty = &mut *pty_guard;
        let mut pty_output_file = unlocked_pty.file.try_clone().unwrap();
        drop(pty_guard);

        let terminal_arc = new_tab.terminal.clone();
        
        let mut tabs_guard = self.tabs.write().unwrap();
        let tabs = &mut *tabs_guard;
        tabs.push(new_tab);
        drop(tabs_guard);

        if self.num_tabs() == 1 {
            self.set_selected_tab(0);
        }

        info!("Inserted and selected new tab {}\n", tab_idx);

        let event_proxy_clone = self.event_proxy.clone();
        thread::spawn( move || {
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

                            buffer.iter().for_each(|byte| {
                                processor.advance(terminal, *byte, &mut unlocked_pty)
                            });

                            drop(pty_guard);
                            drop(terminal_guard);
                        }

                        if rlen == 0 {
                            // Close this tty
                            event_proxy_clone.send_event(alacritty_terminal::event::Event::Close(tab_idx));
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
        *(&mut *wg) = Some(idx);
        drop(wg);
    }

    pub fn remove_selected_tab(&self) {
        match self.selected_tab_idx() {
            Some(idx) => {
                self.tabs.write().unwrap().remove(idx);
            },
            None => {},
        };

        if self.num_tabs() == 0 {
            match self.new_tab() {
                Ok(_idx) => {
                    self.set_selected_tab(0);
                },
                Err(e) => {
                    error!("Attempted to remove a tab when no tabs exist: {}", e);
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

    pub fn remove_tab(&self, idx: usize) {
        if self.num_tabs() > idx {
            self.tabs.write().unwrap().remove(idx);
        }

        if self.num_tabs() == 0 {
            match self.new_tab() {
                Ok(_idx) => {
                    self.set_selected_tab(0);
                },
                Err(e) => {
                    error!("Attempted to remove a tab when no tabs exist: {}", e);
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

    pub fn get_selected_tab_terminal(&self) -> Arc<FairMutex<Term<T>>> {
        self.selected_tab_arc().terminal.clone()
    }

    fn selected_tab_arc(&self) -> Arc<Tab<T>> {
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
                            error!("Error creating new tab: {}", e);
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
}

#[derive(Clone)]
pub struct Tab<T> {
    pub pty: Arc<FairMutex<ChildPty>>,
    pub terminal: Arc<FairMutex<Term<T>>>,
}

impl<T: Clone + EventListener> Tab<T> {
    pub fn new(
        command: &str,
        size: SizeInfo,
        config: Config,
        event_proxy: T,
    ) -> Tab<T> {
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
}
