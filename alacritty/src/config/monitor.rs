use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use log::{debug, error};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use winit::event_loop::EventLoopProxy;

use alacritty_terminal::thread;

use crate::event::{Event, EventType};

const DEBOUNCE_DELAY: Duration = Duration::from_millis(10);

/// The fallback for `RecommendedWatcher` polling.
const FALLBACK_POLLING_TIMEOUT: Duration = Duration::from_secs(1);

pub fn watch(mut paths: Vec<PathBuf>, event_proxy: EventLoopProxy<Event>) {
    // Don't monitor config if there is no path to watch.
    if paths.is_empty() {
        return;
    }

    // Exclude char devices like `/dev/null`, sockets, and so on, by checking that file type is a
    // regular file.
    paths.retain(|path| {
        // Call `metadata` to resolve symbolic links.
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
    let mut watcher = match RecommendedWatcher::new(
        tx,
        Config::default().with_poll_interval(FALLBACK_POLLING_TIMEOUT),
    ) {
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
            if let Err(err) = watcher.watch(parent, RecursiveMode::NonRecursive) {
                debug!("Unable to watch config directory {:?}: {}", parent, err);
            }
        }

        // The current debouncing time.
        let mut debouncing_deadline: Option<Instant> = None;

        // The events accumulated during the debounce period.
        let mut received_events = Vec::new();

        loop {
            // We use `recv_timeout` to debounce the events coming from the watcher and reduce
            // the amount of config reloads.
            let event = match debouncing_deadline.as_ref() {
                Some(debouncing_deadline) => {
                    rx.recv_timeout(debouncing_deadline.saturating_duration_since(Instant::now()))
                },
                None => {
                    let event = rx.recv().map_err(Into::into);
                    // Set the debouncing deadline after receiving the event.
                    debouncing_deadline = Some(Instant::now() + DEBOUNCE_DELAY);
                    event
                },
            };

            match event {
                Ok(Ok(event)) => match event.kind {
                    EventKind::Any
                    | EventKind::Create(_)
                    | EventKind::Modify(_)
                    | EventKind::Other => {
                        received_events.push(event);
                    },
                    _ => (),
                },
                Err(RecvTimeoutError::Timeout) => {
                    // Go back to polling the events.
                    debouncing_deadline = None;

                    if received_events
                        .drain(..)
                        .flat_map(|event| event.paths.into_iter())
                        .any(|path| paths.contains(&path))
                    {
                        // Always reload the primary configuration file.
                        let event = Event::new(EventType::ConfigReload(paths[0].clone()), None);
                        let _ = event_proxy.send_event(event);
                    }
                },
                Ok(Err(err)) => {
                    debug!("Config watcher errors: {:?}", err);
                },
                Err(err) => {
                    debug!("Config watcher channel dropped unexpectedly: {}", err);
                    break;
                },
            };
        }
    });
}
