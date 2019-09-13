use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};

pub struct Monitor {
    _thread: ::std::thread::JoinHandle<()>,
    rx: mpsc::Receiver<PathBuf>,
}

pub trait OnConfigReload {
    fn on_config_reload(&mut self);
}

impl OnConfigReload for crate::display::Notifier {
    fn on_config_reload(&mut self) {
        self.notify();
    }
}

impl Monitor {
    /// Get pending config changes
    pub fn pending(&self) -> Option<PathBuf> {
        let mut config = None;
        while let Ok(new) = self.rx.try_recv() {
            config = Some(new);
        }

        config
    }

    pub fn new<H, P>(path: P, mut handler: H) -> Monitor
    where
        H: OnConfigReload + Send + 'static,
        P: Into<PathBuf>,
    {
        let path = path.into();

        let (config_tx, config_rx) = mpsc::channel();

        Monitor {
            _thread: crate::util::thread::spawn_named("config watcher", move || {
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

                            let _ = config_tx.send(path);
                            handler.on_config_reload();
                        },
                        _ => {},
                    }
                }
            }),
            rx: config_rx,
        }
    }
}
