use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::util;

use crate::event::EventProxy;

pub struct Monitor {
    _thread: ::std::thread::JoinHandle<()>,
}

impl Monitor {
    pub fn new<P>(path: P, event_proxy: EventProxy) -> Monitor
    where
        P: Into<PathBuf>,
    {
        let path = path.into();

        Monitor {
            _thread: util::thread::spawn_named("config watcher", move || {
                let (tx, rx) = mpsc::channel();
                // The Duration argument is a debouncing period.
                let mut watcher =
                    watcher(tx, Duration::from_millis(10)).expect("Unable to spawn file watcher");
                let config_path = ::std::fs::canonicalize(path).expect("canonicalize config path");

                // Get directory of config
                let mut parent = config_path.clone();
                parent.pop();

                // Watch directory
                watcher
                    .watch(&parent, RecursiveMode::NonRecursive)
                    .expect("watch alacritty.yml dir");

                loop {
                    match rx.recv().expect("watcher event") {
                        DebouncedEvent::Rename(..) => continue,
                        DebouncedEvent::Write(path)
                        | DebouncedEvent::Create(path)
                        | DebouncedEvent::Chmod(path) => {
                            if path != config_path {
                                continue;
                            }

                            event_proxy.send_event(Event::ConfigReload(path));
                        },
                        _ => {},
                    }
                }
            }),
        }
    }
}
