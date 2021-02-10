// #[cfg(not(target_os = "windows"))]
// use libc::winsize;


use std::{
    thread,
};

use anyhow::Result;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use std::panic;

pub const DEFAULT_SHELL: &str = "/bin/zsh";

use log::{error, info};

use pad::PadStr;

use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;

use std::io::Read;

use alacritty_terminal::term::SizeInfo;

use crate::child_pty::Pty;

use alacritty_terminal::event::EventListener;

use crate::config::Config;


const TAB_TITLE_WIDTH: usize = 8;

pub struct TabManager<T> {
    pub to_exit: RwLock<bool>,
    pub selected_tab: RwLock<Option<usize>>,
    pub tabs: RwLock<Vec<Tab<T>>>,
    pub size: RwLock<Option<SizeInfo>>,
    pub event_proxy: T,
    pub config: Config,
    pub last_update: std::time::Instant,
    pub tab_titles: RwLock<Vec<String>>,
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
            tab_titles: RwLock::new(Vec::new()),
        }
    }

    pub fn resize(&self, sz: SizeInfo) {
        loop {
            if let Ok(mut size_write_guard) = self.size.try_write() {
                *size_write_guard = Some(sz);
                break;
            }
        }
        loop {
            if let Ok(tabs_read_guard) = self.tabs.try_read() {
                for tab in (*tabs_read_guard).iter() {
                    let terminal_mutex = tab.terminal.clone();
                    let mut terminal_guard = terminal_mutex.lock();
                    let terminal = &mut *terminal_guard;
                    let term_sz = sz;
                    terminal.resize(term_sz);
                    drop(terminal_guard);

                    let pty_mutex = tab.pty.clone();
                    let mut pty_guard = pty_mutex.lock();
                    let pty = &mut *pty_guard;
                    let pty_sz = sz;
                    pty.on_resize(&pty_sz);
                    drop(pty_guard);
                }
                break;
            }
        }
    }

    pub fn set_size(&self, sz: SizeInfo) {
        loop {
            if let Ok(mut size_write_guard) = self.size.try_write() {
                *size_write_guard = Some(sz);
                break;
            }
        }
    }

    #[inline]
    pub fn num_tabs(&self) -> usize {
        loop {
            if let Ok(tabs_read_guard) = self.tabs.try_read() {
                return (*tabs_read_guard).len();
            }
        }
    }

    pub fn new_tab(&self) -> Result<usize> {
        let tab_idx = match self.selected_tab_idx() {
            Some(idx) => idx + 1,
            None => 0,
        };
        info!("Creating new tab {}\n", tab_idx);
        info!("Default shell {}\n", DEFAULT_SHELL);

        let size_info_option: Option<SizeInfo>;
        loop {
            if let Ok(size_read_guard) = self.size.try_read() {
                size_info_option = *size_read_guard;
                break;
            }
        }
        if size_info_option.is_none() {
            panic!("Unable to read the current terminal size");
        };

        let sz = size_info_option.unwrap();
        let new_tab = Tab::new(
            sz,
            self.config.clone(),
            self.event_proxy.clone(),
        );

        let pty_arc = new_tab.pty.clone();
        let mut pty_guard = pty_arc.lock();
        let unlocked_pty = &mut *pty_guard;
        let mut pty_output_file = unlocked_pty.fin_clone();
        drop(pty_guard);

        let terminal_arc = new_tab.terminal.clone();

        loop {
            if let Ok(mut tabs_write_guard) = self.tabs.try_write() {
                (*tabs_write_guard).push(new_tab);
                break;
            }
        }


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
                            let terminal = &mut *terminal_guard;
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
                            event_proxy_clone.send_event(alacritty_terminal::event::Event::Close);
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
        loop {
            if let Ok(mut write_guard) = self.selected_tab.try_write() {
                *write_guard = Some(idx);
                break;
            }
        }
    }

    pub fn remove_selected_tab(&self) {
        if let Some(idx) = self.selected_tab_idx() {
            loop {
                if let Ok(mut rwlock_tabs) = self.tabs.try_write() {
                    rwlock_tabs.remove(idx);
                    break;
                }
            }
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

    pub fn get_selected_tab_pty(&self) -> Arc<FairMutex<Pty>> {
        self.selected_tab_arc().pty.clone()
    }

    pub fn get_selected_tab_terminal(&self) -> Arc<FairMutex<Term<T>>> {
        self.selected_tab_arc().terminal.clone()
    }

    pub fn update_tab_titles(&self) {
        let mut tab_idx: usize = 0;
        loop {
            if let Ok(all_cur_tabs_guard) = self.tabs.try_read() {
                let all_cur_tabs = &*all_cur_tabs_guard;

                let tabs_count = all_cur_tabs.len();
                if tabs_count == 0 {
                    loop {
                        if let Ok(mut tab_titles_guard) = self.tab_titles.try_write() {
                            let tab_titles = &mut *tab_titles_guard;
                            *tab_titles = Vec::new();
                            break;
                        }
                    }
                } else {
                    let selected_tab_idx: usize;
                    loop {
                        if let Ok(tab_idx_guard) = self.selected_tab.try_read() {
                            let tab_idx_option = *tab_idx_guard;
                            if let Some(idx) = tab_idx_option {
                                selected_tab_idx = idx;
                                break;
                            }
                        }
                    }

                    loop {
                        if let Ok(mut tab_titles_guard) = self.tab_titles.try_write() {
                            let tab_titles = &mut *tab_titles_guard;
                            *tab_titles = all_cur_tabs.iter().map(|cur_tab| {
                                let term_guard = cur_tab.terminal.lock();
                                let term = &*term_guard;
                                let formatted_title: String;

                                let selected_tab_char = if tab_idx == selected_tab_idx { "*".to_string() } else { "".to_string() };

                                if  let Some(actual_title_string) = &term.title {
                                    if actual_title_string.len() > TAB_TITLE_WIDTH {
                                        formatted_title = actual_title_string[(actual_title_string.len() - 8)..].to_string()
                                    } else {
                                        formatted_title = actual_title_string.with_exact_width(TAB_TITLE_WIDTH);
                                    }
        
                                } else {
                                    // let temp_formatted_title = format!("[*{:0>8}]", tab_idx);
                                    let temp_formatted_title = format!("{}", tab_idx);
                                    formatted_title = temp_formatted_title.pad(TAB_TITLE_WIDTH, ' ', pad::Alignment::Left, true)
                                }

                                let final_formatted_title = format!("{}{}", selected_tab_char, formatted_title);
        
                                tab_idx += 1;
                                final_formatted_title
                            }).collect();
                            break;
                        }
                    }
                }
                break;
            }
        }
    }

    fn selected_tab_arc(&self) -> Arc<Tab<T>> {
        match self.selected_tab_idx() {
            Some(sel_idx) => {
                loop {
                    if let Ok(tabs_guard) = self.tabs.try_read() {
                        let tabs = & *tabs_guard;
                        let tab = tabs.get(sel_idx).unwrap();
                        let tab_clone = tab.clone();
                        return Arc::new(tab_clone);
                    }
                }
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
                loop {
                    if let Ok(tabs_guard) = self.tabs.try_read() {
                        let tabs = & *tabs_guard;
                        let tab = tabs.get(0).unwrap().clone();
                        return Arc::new(tab);
                    }
                }
            },
        }
    }

    pub fn select_tab(& self, idx: usize) -> Option<usize> {
        self.set_selected_tab(idx);
        self.selected_tab_idx()
    }

    #[inline]
    pub fn selected_tab_idx(&self) -> Option<usize> {
        loop {
            if let Ok(selected_tab_guard) = self.selected_tab.try_read() {
                return *selected_tab_guard;
            }
        }
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
    pub pty: Arc<FairMutex<Pty>>,
    pub terminal: Arc<FairMutex<Term<T>>>,
}

impl<T: Clone + EventListener> Tab<T> {
    pub fn new(
        size: SizeInfo,
        config: Config,
        event_proxy: T,
    ) -> Tab<T> {
        let terminal = Term::new(&config, size, event_proxy);
        let terminal = Arc::new(FairMutex::new(terminal));

        let pty = Arc::new(FairMutex::new(crate::child_pty::new(config, size).unwrap()));

        Tab { pty, terminal }
    }
}
