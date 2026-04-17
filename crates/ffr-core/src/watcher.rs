//! Per-file watcher for cache invalidation.
//!
//! Unlike fff's background_watcher (which walks a project tree), ffr watches
//! exactly the set of files that have been classified/cached. On a Modify
//! or Remove event it clears:
//!   - the ephemeral LineIndex cache entry for that path
//!   - the persistent LMDB metadata entry for that path
//!
//! Architecture mirrors fff-core::background_watcher: the debouncer is owned
//! by a dedicated thread that we join on `stop()`. The callback handles
//! events via the `SharedMetadata` + `SharedLineIndexCache` handles, so no
//! global state is required.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, NoCache, new_debouncer_opt};
use tracing::{debug, error, info, warn};

use crate::cache;
use crate::errors::FFRError;
use crate::shared::{SharedLineIndexCache, SharedMetadata};

type Debouncer = notify_debouncer_full::Debouncer<notify::RecommendedWatcher, NoCache>;

const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(250);

/// File-system watcher. Drop this (or call `stop()`) to tear down the
/// underlying thread.
pub struct FileWatcher {
    stop_signal: Arc<AtomicBool>,
    owner_thread: Option<std::thread::JoinHandle<()>>,
    watched: Arc<Mutex<HashSet<PathBuf>>>,
    debouncer: Arc<Mutex<Option<Debouncer>>>,
}

impl FileWatcher {
    /// Spawn a watcher that invalidates cache entries on Modify/Remove events.
    pub fn spawn(
        metadata: SharedMetadata,
        line_cache: SharedLineIndexCache,
        debounce: Option<Duration>,
    ) -> Result<Self, FFRError> {
        let debounce = debounce.unwrap_or(DEFAULT_DEBOUNCE);
        let watched: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));
        let watched_cb = Arc::clone(&watched);

        let handler = move |res: DebounceEventResult| match res {
            Err(errs) => {
                for e in errs {
                    error!(error = %e, "ffr watcher error");
                }
            }
            Ok(events) => {
                let watched_now = match watched_cb.lock() {
                    Ok(g) => g.clone(),
                    Err(_) => return,
                };
                for ev in events {
                    match ev.event.kind {
                        EventKind::Modify(_) | EventKind::Remove(_) | EventKind::Create(_) => {
                            for p in &ev.event.paths {
                                if !watched_now.contains(p) {
                                    continue;
                                }
                                let _ = cache::invalidate_line_index_for(p);
                                let key = p.to_string_lossy().to_string();
                                if let Err(e) = cache::remove_metadata_entry(&key) {
                                    debug!(error = %e, path = %p.display(), "remove_metadata_entry failed");
                                }
                                if let Err(e) = cache::remove_semantic(&key) {
                                    debug!(error = %e, path = %p.display(), "remove_semantic failed");
                                }
                                info!(path = %p.display(), "invalidated cache on fs event");
                            }
                        }
                        _ => {}
                    }
                }
            }
        };

        let config = notify::Config::default().with_poll_interval(Duration::from_secs(2));
        let debouncer = new_debouncer_opt::<_, notify::RecommendedWatcher, NoCache>(
            debounce, None, handler, NoCache, config,
        )
        .map_err(|e| FFRError::Internal(format!("start watcher: {e}")))?;

        let debouncer = Arc::new(Mutex::new(Some(debouncer)));
        let debouncer_owner = Arc::clone(&debouncer);

        let stop_signal = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop_signal);

        let owner_thread = std::thread::Builder::new()
            .name("ffr-watcher-owner".into())
            .spawn(move || {
                while !stop_clone.load(Ordering::Acquire) {
                    std::thread::park_timeout(Duration::from_secs(1));
                }
                if let Ok(mut guard) = debouncer_owner.lock() {
                    if let Some(d) = guard.take() {
                        d.stop();
                    }
                }
            })
            .map_err(|e| FFRError::Internal(format!("spawn watcher thread: {e}")))?;

        // Keep the shared handles alive for the lifetime of the watcher so
        // `cache::*` calls above see an initialized singleton. These clones
        // do nothing beyond holding the Arc's refcount, but prevent a caller
        // from accidentally dropping both handles and causing the callback
        // to lock against a dead store.
        drop(metadata);
        drop(line_cache);

        Ok(FileWatcher {
            stop_signal,
            owner_thread: Some(owner_thread),
            watched,
            debouncer,
        })
    }

    /// Start watching `path`. Non-recursive (watches the file itself).
    pub fn watch(&self, path: &Path) -> Result<(), FFRError> {
        let mut guard = self
            .debouncer
            .lock()
            .map_err(|_| FFRError::Internal("watcher lock poisoned".to_string()))?;
        let d = guard
            .as_mut()
            .ok_or_else(|| FFRError::Internal("watcher stopped".to_string()))?;
        d.watch(path, RecursiveMode::NonRecursive)
            .map_err(|e| FFRError::Internal(format!("watch {}: {e}", path.display())))?;
        drop(guard);

        let mut w = self
            .watched
            .lock()
            .map_err(|_| FFRError::Internal("watched set poisoned".to_string()))?;
        w.insert(path.to_path_buf());
        Ok(())
    }

    /// Stop watching `path`.
    pub fn unwatch(&self, path: &Path) -> Result<(), FFRError> {
        let mut guard = self
            .debouncer
            .lock()
            .map_err(|_| FFRError::Internal("watcher lock poisoned".to_string()))?;
        if let Some(d) = guard.as_mut() {
            if let Err(e) = d.unwatch(path) {
                warn!(error = %e, path = %path.display(), "unwatch failed");
            }
        }
        drop(guard);
        let mut w = self
            .watched
            .lock()
            .map_err(|_| FFRError::Internal("watched set poisoned".to_string()))?;
        w.remove(path);
        Ok(())
    }

    /// Return a snapshot of currently watched paths.
    pub fn watched_paths(&self) -> Vec<PathBuf> {
        self.watched
            .lock()
            .map(|g| g.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Stop the watcher and join the owner thread.
    pub fn stop(&mut self) {
        self.stop_signal.store(true, Ordering::Release);
        if let Some(t) = self.owner_thread.take() {
            t.thread().unpark();
            let _ = t.join();
        }
    }
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    #[test]
    fn spawn_and_stop() {
        let metadata = SharedMetadata::default();
        let line_cache = SharedLineIndexCache::default();
        let mut w = FileWatcher::spawn(metadata, line_cache, Some(Duration::from_millis(50)))
            .expect("spawn");
        assert!(w.watched_paths().is_empty());
        w.stop();
    }

    #[test]
    fn watch_registers_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let file = tmp.path().join("x.txt");
        fs::write(&file, "hello\n").unwrap();

        let metadata = SharedMetadata::default();
        let line_cache = SharedLineIndexCache::default();
        let w = FileWatcher::spawn(metadata, line_cache, Some(Duration::from_millis(50)))
            .expect("spawn");
        w.watch(&file).unwrap();
        let paths = w.watched_paths();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], file);
        w.unwatch(&file).unwrap();
        assert!(w.watched_paths().is_empty());
    }
}
