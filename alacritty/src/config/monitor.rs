use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};

use alacritty_terminal::thread;

use crate::event::{Event, EventProxy};

pub struct Monitor {
    _thread: ::std::thread::JoinHandle<()>,
}

impl Monitor {
    pub fn new(paths: Vec<PathBuf>, event_proxy: EventProxy) -> Monitor {
        Monitor {
            _thread: thread::spawn_named("config watcher", move || {
                let (tx, rx) = mpsc::channel();
                // The Duration argument is a debouncing period.
                let mut watcher =
                    watcher(tx, Duration::from_millis(10)).expect("Unable to spawn file watcher");

                // Get all unique parent directories.
                let mut parents = paths
                    .iter()
                    .map(|path| {
                        let mut path = fs::canonicalize(path).expect("canonicalize config path");
                        path.pop();
                        path
                    })
                    .collect::<Vec<PathBuf>>();
                parents.sort_unstable();
                parents.dedup();

                // Watch all configuration file directories.
                for parent in &parents {
                    watcher
                        .watch(&parent, RecursiveMode::NonRecursive)
                        .expect("watch alacritty.yml dir");
                }

                loop {
                    match rx.recv().expect("watcher event") {
                        DebouncedEvent::Rename(..) => continue,
                        DebouncedEvent::Write(path)
                        | DebouncedEvent::Create(path)
                        | DebouncedEvent::Chmod(path) => {
                            if !paths.contains(&path) {
                                continue;
                            }

                            // Always reload the primary configuration file.
                            event_proxy.send_event(Event::ConfigReload(paths[0].clone()));
                        },
                        _ => {},
                    }
                }
            }),
        }
    }
}
