//! Ephemeral LineIndex cache + persistent metadata (LMDB) facade.
//!
//! The module-level `SharedMetadata` / `SharedLineIndexCache` statics are the
//! process-wide handles used by `ffr-nvim` and `ffr-mcp`. Tests and other
//! callers can construct their own handles via `shared::*::default()`.

use std::path::Path;
use std::sync::OnceLock;

use crate::db::{MetadataDb, SemanticRecord};
use crate::errors::FFRError;
use crate::index::MetadataIndexEntry;
use crate::lines::LineIndex;
use crate::shared::{SharedLineIndexCache, SharedMetadata};

// ─── Process-wide handles ──────────────────────────────────────────────

fn metadata_handle() -> &'static SharedMetadata {
    static H: OnceLock<SharedMetadata> = OnceLock::new();
    H.get_or_init(SharedMetadata::default)
}

fn line_handle() -> &'static SharedLineIndexCache {
    static H: OnceLock<SharedLineIndexCache> = OnceLock::new();
    H.get_or_init(SharedLineIndexCache::default)
}

/// Public getter for the process metadata handle. Watchers and health
/// checks clone this to hold a reference without taking the lock.
pub fn shared_metadata() -> SharedMetadata {
    metadata_handle().clone()
}

/// Public getter for the process line-index cache handle.
pub fn shared_line_index_cache() -> SharedLineIndexCache {
    line_handle().clone()
}

// ─── Line-index cache (ephemeral) ──────────────────────────────────────

pub fn get_line_index(path: &Path) -> Result<LineIndex, FFRError> {
    let key = line_key_for(path)?;

    {
        let map = line_handle().read()?;
        if let Some(idx) = map.get(&key) {
            return Ok(idx.clone());
        }
    }

    let idx = crate::lines::construct_line_index(path)?;

    {
        let mut map = line_handle().write()?;
        map.insert(key, idx.clone());
    }
    Ok(idx)
}

fn line_key_for(path: &Path) -> Result<String, FFRError> {
    let metadata = path.metadata()?;
    let size = metadata.len();
    let mtime = metadata
        .modified()
        .map_err(|e| FFRError::IOError(e.to_string()))?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| FFRError::IOError(e.to_string()))?
        .as_secs();
    Ok(format!("{}::{}:{}", path.to_string_lossy(), size, mtime))
}

pub fn clear_line_indexes() -> Result<(), FFRError> {
    let mut map = line_handle().write()?;
    map.clear();
    Ok(())
}

/// Invalidate a single file in the ephemeral line cache. Called by the
/// file watcher when the underlying file changes on disk.
pub fn invalidate_line_index_for(path: &Path) -> Result<(), FFRError> {
    let prefix = format!("{}::", path.to_string_lossy());
    let mut map = line_handle().write()?;
    map.retain(|k, _| !k.starts_with(&prefix));
    Ok(())
}

// ─── Metadata (persistent, LMDB) ───────────────────────────────────────

/// Open the LMDB metadata store at `path` and install it as the process
/// singleton. If `path` points to a legacy `.json` file, the JSON is
/// migrated into a sibling `*-db/` directory on first open.
pub fn load_metadata_index(path: &str) -> Result<(), FFRError> {
    let db = MetadataDb::open(path)?;
    metadata_handle().init(db)?;
    Ok(())
}

/// LMDB commits on every upsert/delete, so `save_metadata_index` is a no-op
/// retained for FFI backward compatibility (callers may invoke on shutdown).
pub fn save_metadata_index() -> Result<(), FFRError> {
    Ok(())
}

pub fn get_metadata_entry(file_path: &str) -> Result<Option<MetadataIndexEntry>, FFRError> {
    let guard = metadata_handle().read()?;
    match guard.as_ref() {
        Some(db) => db.get(file_path),
        None => Ok(None),
    }
}

pub fn upsert_metadata_entry(entry: MetadataIndexEntry) -> Result<(), FFRError> {
    let guard = metadata_handle().read()?;
    match guard.as_ref() {
        Some(db) => db.upsert(&entry),
        None => Ok(()),
    }
}

pub fn remove_metadata_entry(file_path: &str) -> Result<bool, FFRError> {
    let guard = metadata_handle().read()?;
    match guard.as_ref() {
        Some(db) => db.remove(file_path),
        None => Ok(false),
    }
}

pub fn metadata_count() -> Result<u64, FFRError> {
    let guard = metadata_handle().read()?;
    match guard.as_ref() {
        Some(db) => db.count(),
        None => Ok(0),
    }
}

pub fn metadata_path() -> Result<Option<String>, FFRError> {
    let guard = metadata_handle().read()?;
    Ok(guard
        .as_ref()
        .map(|db| db.path().to_string_lossy().into_owned()))
}

pub fn metadata_disk_size() -> Result<u64, FFRError> {
    let guard = metadata_handle().read()?;
    match guard.as_ref() {
        Some(db) => db.disk_size(),
        None => Ok(0),
    }
}

pub fn clear_all() -> Result<(), FFRError> {
    clear_line_indexes()?;
    let guard = metadata_handle().read()?;
    if let Some(db) = guard.as_ref() {
        db.clear()?;
        db.clear_semantic()?;
    }
    Ok(())
}

// ─── Semantic chunk cache (persistent) ───────────────────────────────

pub fn get_semantic(path: &str) -> Result<Option<SemanticRecord>, FFRError> {
    let guard = metadata_handle().read()?;
    match guard.as_ref() {
        Some(db) => db.get_semantic(path),
        None => Ok(None),
    }
}

pub fn upsert_semantic(path: &str, record: &SemanticRecord) -> Result<(), FFRError> {
    let guard = metadata_handle().read()?;
    match guard.as_ref() {
        Some(db) => db.upsert_semantic(path, record),
        None => Ok(()),
    }
}

pub fn remove_semantic(path: &str) -> Result<bool, FFRError> {
    let guard = metadata_handle().read()?;
    match guard.as_ref() {
        Some(db) => db.remove_semantic(path),
        None => Ok(false),
    }
}

pub fn clear_semantic() -> Result<(), FFRError> {
    let guard = metadata_handle().read()?;
    if let Some(db) = guard.as_ref() {
        db.clear_semantic()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_line_key_for_creates_key() {
        let tmp = std::env::temp_dir().join("ffr_cache_test_key.txt");
        std::fs::write(&tmp, "hello\nworld\n").unwrap();
        let key = line_key_for(&tmp).unwrap();
        assert!(key.contains("ffr_cache_test_key.txt"));
        assert!(key.contains("::"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_line_index_cache_reuse_same_mtime() {
        let tmp = std::env::temp_dir().join("ffr_cache_reuse.txt");
        std::fs::write(&tmp, "aaa\nbbb\nccc\n").unwrap();

        let idx1 = get_line_index(&tmp).unwrap();
        assert_eq!(idx1.line_count, 3);

        let idx2 = get_line_index(&tmp).unwrap();
        assert_eq!(idx2.line_count, 3);
        assert_eq!(idx1.offsets, idx2.offsets);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_line_index_cache_invalidates_on_content_change() {
        let tmp = std::env::temp_dir().join("ffr_cache_inval.txt");
        std::fs::write(&tmp, "aaa\nbbb\n").unwrap();

        let idx1 = get_line_index(&tmp).unwrap();
        assert_eq!(idx1.line_count, 2);

        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&tmp, "aaa\nbbb\nccc\nddd\n").unwrap();

        let idx2 = get_line_index(&tmp).unwrap();
        assert_eq!(idx2.line_count, 4);
        assert_ne!(idx1.line_count, idx2.line_count);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_metadata_roundtrip_via_handle() {
        // Use isolated TempDir and a fresh SharedMetadata so we don't collide
        // with the process-wide singleton used by other tests.
        let tmp = TempDir::new().unwrap();
        let db = MetadataDb::open(tmp.path()).unwrap();
        let handle = SharedMetadata::default();
        handle.init(db).unwrap();

        let entry = MetadataIndexEntry {
            path: "/test/cache_test.rs".to_string(),
            size: 512,
            mtime: 1700000001,
            revision: "512:1700000001".to_string(),
            binary: false,
            encoding: Some("utf-8".to_string()),
            line_count: Some(20),
            line_index_ready: false,
            last_validated: 1700000001,
        };

        {
            let guard = handle.read().unwrap();
            guard.as_ref().unwrap().upsert(&entry).unwrap();
        }

        let guard = handle.read().unwrap();
        let got = guard
            .as_ref()
            .unwrap()
            .get("/test/cache_test.rs")
            .unwrap()
            .unwrap();
        assert_eq!(got.size, 512);
    }

    #[test]
    fn test_invalidate_line_index_for() {
        let tmp = std::env::temp_dir().join("ffr_cache_invalidate.txt");
        std::fs::write(&tmp, "x\ny\n").unwrap();
        let _ = get_line_index(&tmp).unwrap();
        invalidate_line_index_for(&tmp).unwrap();
        // next call rebuilds — no assertion on identity possible, but we can
        // verify the cache is empty for this path by checking internals:
        let prefix = format!("{}::", tmp.to_string_lossy());
        let map = line_handle().read().unwrap();
        assert!(map.keys().all(|k| !k.starts_with(&prefix)));
        let _ = std::fs::remove_file(&tmp);
    }
}
