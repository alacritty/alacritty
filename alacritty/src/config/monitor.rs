use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use log::{debug, error, warn};
use notify::event::{ModifyKind, RenameMode};
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
    watched_hash: Option<u64>,
}

impl ConfigMonitor {
    pub fn new(mut paths: Vec<PathBuf>, event_proxy: EventLoopProxy<Event>) -> Option<Self> {
        // Don't monitor config if there is no path to watch.
        if paths.is_empty() {
            return None;
        }

        // Calculate the hash for the unmodified list of paths.
        let watched_hash = Self::hash_paths(&paths);

        // Exclude char devices like `/dev/null`, sockets, and so on, by checking that file type is
        // a regular file.
        paths.retain(|path| {
            // Call `metadata` to resolve symbolic links.
            path.metadata().is_ok_and(|metadata| metadata.file_type().is_file())
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
                error!("Unable to watch config file: {err}");
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
                    debug!("Unable to watch config directory {parent:?}: {err}");
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
                        // Ignore when config file is moved as it's equivalent to deletion.
                        // Some editors trigger this as they move the file as part of saving.
                        EventKind::Modify(ModifyKind::Name(
                            RenameMode::From | RenameMode::Both,
                        )) => (),
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
                        debug!("Config watcher errors: {err:?}");
                    },
                    Err(err) => {
                        debug!("Config watcher channel dropped unexpectedly: {err}");
                        break;
                    },
                };
            }
        });

        Some(Self { watched_hash, thread: join_handle, shutdown_tx: tx })
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

    /// Check if the config monitor needs to be restarted.
    ///
    /// This checks the supplied list of files against the monitored files to determine if a
    /// restart is necessary.
    pub fn needs_restart(&self, files: &[PathBuf]) -> bool {
        Self::hash_paths(files).is_none_or(|hash| Some(hash) == self.watched_hash)
    }

    /// Generate the hash for a list of paths.
    fn hash_paths(files: &[PathBuf]) -> Option<u64> {
        // Use file count limit to avoid allocations.
        const MAX_PATHS: usize = 1024;
        if files.len() > MAX_PATHS {
            return None;
        }

        // Sort files to avoid restart on order change.
        let mut sorted_files = [None; MAX_PATHS];
        for (i, file) in files.iter().enumerate() {
            sorted_files[i] = Some(file);
        }
        sorted_files.sort_unstable();

        // Calculate hash for the paths, regardless of order.
        let mut hasher = DefaultHasher::new();
        Hash::hash_slice(&sorted_files, &mut hasher);
        Some(hasher.finish())
    }
}
