use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use log::{debug, error};
use notify_debouncer_mini::new_debouncer;
use notify_debouncer_mini::notify::RecursiveMode;
use winit::event_loop::EventLoopProxy;

use alacritty_terminal::thread;

use crate::event::{Event, EventType};

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
const DEBOUNCE_DELAY: Duration = Duration::from_millis(10);
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const DEBOUNCE_DELAY: Duration = Duration::from_millis(1000);

pub fn watch(mut paths: Vec<PathBuf>, event_proxy: EventLoopProxy<Event>) {
    // Don't monitor config if there is no path to watch.
    if paths.is_empty() {
        return;
    }

    // Exclude char devices like `/dev/null`, sockets, and so on, by checking that file type is a
    // regular file.
    paths.retain(|path| {
        // Call `metadata` to resolve symbolink links.
        path.metadata().map_or(false, |metadata| metadata.file_type().is_file())
    });

    // Canonicalize paths, keeping the base paths for symlinks.
    for i in 0..paths.len() {
        if let Ok(canonical_path) = paths[i].canonicalize() {
            match paths[i].symlink_metadata() {
                Ok(metadata) if metadata.file_type().is_symlink() => paths.push(canonical_path),
                _ => paths[i] = canonical_path,
            }
        }
    }

    // The Duration argument is a debouncing period.
    let (tx, rx) = mpsc::channel();
    let mut debouncer = match new_debouncer(DEBOUNCE_DELAY, None, tx) {
        Ok(debouncer) => debouncer,
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

        let watcher = debouncer.watcher();
        // Watch all configuration file directories.
        for parent in &parents {
            if let Err(err) = watcher.watch(parent, RecursiveMode::NonRecursive) {
                debug!("Unable to watch config directory {:?}: {}", parent, err);
            }
        }

        loop {
            let events = match rx.recv() {
                Ok(Ok(events)) => events,
                Ok(Err(err)) => {
                    debug!("Config watcher errors: {:?}", err);
                    continue;
                },
                Err(err) => {
                    debug!("Config watcher channel dropped unexpectedly: {}", err);
                    break;
                },
            };

            if events.iter().any(|e| paths.contains(&e.path)) {
                // Always reload the primary configuration file.
                let event = Event::new(EventType::ConfigReload(paths[0].clone()), None);
                let _ = event_proxy.send_event(event);
            }
        }
    });
}
