use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use log::{debug, error, warn};
use notify::{
    Config, Error as NotifyError, Event as NotifyEvent, EventKind, RecommendedWatcher,
    RecursiveMode, Watcher,
};
use winit::event_loop::EventLoopProxy;

use alacritty_terminal::thread;

use crate::event::{Event, EventType};

const DEBOUNCE_DELAY: Duration = Duration::from_millis(10);

/// The fallback for `RecommendedWatcher` polling.
const FALLBACK_POLLING_TIMEOUT: Duration = Duration::from_secs(1);

/// Config file update monitor.
pub struct ConfigMonitor {
    thread: JoinHandle<()>,
    shutdown_tx: Sender<Result<NotifyEvent, NotifyError>>,
}

impl ConfigMonitor {
    pub fn new(mut paths: Vec<PathBuf>, event_proxy: EventLoopProxy<Event>) -> Option<Self> {
        // Don't monitor config if there is no path to watch.
        if paths.is_empty() {
            return None;
        }

        // Exclude char devices like `/dev/null`, sockets, and so on, by checking that file type is
        // a regular file.
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
            tx.clone(),
            Config::default().with_poll_interval(FALLBACK_POLLING_TIMEOUT),
        ) {
            Ok(watcher) => watcher,
            Err(err) => {
                error!("Unable to watch config file: {}", err);
                return None;
            },
        };

        let join_handle = thread::spawn_named("config watcher", move || {
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
                    Some(debouncing_deadline) => rx.recv_timeout(
                        debouncing_deadline.saturating_duration_since(Instant::now()),
                    ),
                    None => {
                        let event = rx.recv().map_err(Into::into);
                        // Set the debouncing deadline after receiving the event.
                        debouncing_deadline = Some(Instant::now() + DEBOUNCE_DELAY);
                        event
                    },
                };

                match event {
                    Ok(Ok(event)) => match event.kind {
                        EventKind::Other if event.info() == Some("shutdown") => break,
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

        Some(Self { thread: join_handle, shutdown_tx: tx })
    }

    /// Synchronously shut down the monitor.
    pub fn shutdown(self) {
        // Request shutdown.
        let mut event = NotifyEvent::new(EventKind::Other);
        event = event.set_info("shutdown");
        let _ = self.shutdown_tx.send(Ok(event));

        // Wait for thread to terminate.
        if let Err(err) = self.thread.join() {
            warn!("config monitor shutdown failed: {err:?}");
        }
    }
}
