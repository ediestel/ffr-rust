//! Background chunk prefetcher.
//!
//! Users submit a (path, chunk_id) hint and the prefetcher reads the chunk on
//! a worker thread so that the subsequent synchronous `read_chunk` hits the
//! OS page cache. Results are not stored in a shared cache here — the goal
//! is to warm the page cache, not to hold payloads in RAM.

use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, bounded};

use crate::errors::FFRError;
use crate::read;

#[derive(Debug, Clone)]
struct PrefetchReq {
    path: String,
    chunk_id: u64,
    chunk_bytes: usize,
}

struct Prefetcher {
    tx: Sender<PrefetchReq>,
}

fn prefetcher() -> &'static Mutex<Option<Prefetcher>> {
    static P: OnceLock<Mutex<Option<Prefetcher>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(None))
}

/// Spawn the prefetcher worker thread if it hasn't been spawned yet. Idempotent.
pub fn spawn() -> Result<(), FFRError> {
    let mut guard = prefetcher()
        .lock()
        .map_err(|_| FFRError::Internal("prefetcher lock poisoned".to_string()))?;
    if guard.is_some() {
        return Ok(());
    }
    let (tx, rx) = bounded::<PrefetchReq>(64);
    thread::Builder::new()
        .name("ffr-prefetch".into())
        .spawn(move || worker_loop(rx))
        .map_err(|e| FFRError::Internal(format!("spawn prefetch thread: {e}")))?;
    *guard = Some(Prefetcher { tx });
    Ok(())
}

fn worker_loop(rx: Receiver<PrefetchReq>) {
    loop {
        match rx.recv_timeout(Duration::from_secs(3600)) {
            Ok(req) => {
                // Result ignored — we only care about warming the page cache.
                let _ = read::read_chunk(&req.path, req.chunk_id, req.chunk_bytes);
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Request a prefetch of a specific chunk. Non-blocking; silently drops if
/// the worker's queue is full.
pub fn hint(path: &str, chunk_id: u64, chunk_bytes: usize) -> Result<(), FFRError> {
    let guard = prefetcher()
        .lock()
        .map_err(|_| FFRError::Internal("prefetcher lock poisoned".to_string()))?;
    if let Some(p) = guard.as_ref() {
        let req = PrefetchReq {
            path: path.to_string(),
            chunk_id,
            chunk_bytes,
        };
        let _ = p.tx.try_send(req);
    }
    Ok(())
}

/// Prefetch a run of consecutive chunks starting at `start_chunk_id`.
pub fn hint_range(
    path: &str,
    start_chunk_id: u64,
    count: u64,
    chunk_bytes: usize,
) -> Result<(), FFRError> {
    for offset in 0..count {
        hint(path, start_chunk_id + offset, chunk_bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn spawn_idempotent_and_hint_does_not_panic() {
        spawn().unwrap();
        spawn().unwrap();

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("p.txt");
        fs::write(&path, "hello\nworld\n").unwrap();
        hint(path.to_str().unwrap(), 0, 4096).unwrap();
        hint_range(path.to_str().unwrap(), 0, 3, 4096).unwrap();
        // Give the worker a moment to consume.
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
