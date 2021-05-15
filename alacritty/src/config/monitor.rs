use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use log::{debug, error};
use notify::{watcher, DebouncedEvent, RecursiveMode, Watcher};

use alacritty_terminal::thread;

use crate::event::{Event, EventProxy};

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
const DEBOUNCE_DELAY: Duration = Duration::from_millis(10);
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const DEBOUNCE_DELAY: Duration = Duration::from_millis(1000);

pub fn watch(mut paths: Vec<PathBuf>, event_proxy: EventProxy) {
    // Canonicalize all paths, filtering out the ones that do not exist.
    paths = paths
        .drain(..)
        .filter_map(|path| match fs::canonicalize(&path) {
            Ok(path) => Some(path),
            Err(err) => {
                error!("Unable to canonicalize config path {:?}: {}", path, err);
                None
            },
        })
        .collect();

    // Don't monitor config if there is no path to watch.
    if paths.is_empty() {
        return;
    }

    // The Duration argument is a debouncing period.
    let (tx, rx) = mpsc::channel();
    let mut watcher = match watcher(tx, DEBOUNCE_DELAY) {
        Ok(watcher) => watcher,
        Err(err) => {
            error!("Unable to watch config file: {}", err);
            return;
        },
    };

    thread::spawn_named("config watcher", move || {
        // Get all unique parent directories.
        let mut parents = paths
            .iter()
            .map(|path| {
                let mut path = path.clone();
                path.pop();
                path
            })
            .collect::<Vec<PathBuf>>();
        parents.sort_unstable();
        parents.dedup();

        // Watch all configuration file directories.
        for parent in &parents {
            if let Err(err) = watcher.watch(&parent, RecursiveMode::NonRecursive) {
                debug!("Unable to watch config directory {:?}: {}", parent, err);
            }
        }

        loop {
            let event = match rx.recv() {
                Ok(event) => event,
                Err(err) => {
                    debug!("Config watcher channel dropped unexpectedly: {}", err);
                    break;
                },
            };

            match event {
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
    });
}
