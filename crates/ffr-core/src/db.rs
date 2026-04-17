//! LMDB-backed persistent metadata store.
//!
//! Replaces the legacy JSON file at `metadata_cache.json`. One-time migration
//! from JSON is triggered the first time the db is opened with a legacy path.

use std::fs;
use std::path::{Path, PathBuf};

use heed::{
    Database, Env, EnvOpenOptions,
    types::{SerdeBincode, Str},
};
use serde::{Deserialize, Serialize};

use crate::errors::FFRError;
use crate::index::{MetadataIndex, MetadataIndexEntry};

/// One entry from a semantic outline. Matches the shape Lua-side produces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticChunk {
    pub start_line: u64,
    pub end_line: u64,
    pub kind: String,
    pub name: Option<String>,
}

/// Revision-tagged list of semantic chunks for a file. Revision is
/// `"{size}:{mtime}"` — same form as `index::compute_revision`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticRecord {
    pub revision: String,
    pub chunks: Vec<SemanticChunk>,
}

/// Default LMDB map size: 64 MiB. File-reader workloads don't hit millions of
/// entries like a file finder would, so 64 MiB is generous headroom.
const DEFAULT_MAP_SIZE: usize = 64 * 1024 * 1024;

/// Legacy JSON cache filename. If the configured path points to a file with
/// this suffix (or any .json file), we auto-migrate.
const LEGACY_JSON_SUFFIX: &str = ".json";

/// LMDB-backed metadata database.
///
/// Single named database `metadata` keyed by canonical path (UTF-8), values
/// serialized via bincode.
#[derive(Debug)]
pub struct MetadataDb {
    env: Env,
    db: Database<Str, SerdeBincode<MetadataIndexEntry>>,
    semantic: Database<Str, SerdeBincode<SemanticRecord>>,
    path: PathBuf,
}

impl MetadataDb {
    /// Open or create an LMDB env at `db_path` (directory). If the provided
    /// path is a legacy .json file, open the sibling `<stem>-db/` directory
    /// and migrate entries from the JSON file exactly once.
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self, FFRError> {
        let input = db_path.as_ref();

        let (dir, legacy_json) = Self::resolve_paths(input);

        fs::create_dir_all(&dir)
            .map_err(|e| FFRError::IOError(format!("create metadata db dir {dir:?}: {e}")))?;

        let env = unsafe {
            let mut opts = EnvOpenOptions::new();
            opts.map_size(DEFAULT_MAP_SIZE);
            opts.max_dbs(4);
            opts.open(&dir)
                .map_err(|e| FFRError::Internal(format!("lmdb open {dir:?}: {e}")))?
        };

        env.clear_stale_readers()
            .map_err(|e| FFRError::Internal(format!("lmdb clear_stale_readers: {e}")))?;

        let mut wtxn = env
            .write_txn()
            .map_err(|e| FFRError::Internal(format!("lmdb write_txn: {e}")))?;
        let db = env
            .create_database(&mut wtxn, Some("metadata"))
            .map_err(|e| FFRError::Internal(format!("lmdb create db: {e}")))?;
        let semantic = env
            .create_database(&mut wtxn, Some("semantic"))
            .map_err(|e| FFRError::Internal(format!("lmdb create semantic db: {e}")))?;
        wtxn.commit()
            .map_err(|e| FFRError::Internal(format!("lmdb commit: {e}")))?;

        let store = MetadataDb {
            env,
            db,
            semantic,
            path: dir.clone(),
        };

        if let Some(json_path) = legacy_json {
            if json_path.exists() {
                let imported = store.migrate_from_json(&json_path)?;
                if imported > 0 {
                    tracing::info!(count = imported, src = %json_path.display(), "migrated legacy JSON metadata cache to LMDB");
                }
                // delete legacy file so we don't re-migrate on every open
                let _ = fs::remove_file(&json_path);
            }
        }

        Ok(store)
    }

    /// Resolve a caller-provided path into (db_dir, optional legacy JSON path).
    ///
    /// - If the path ends with `.json`: use `<parent>/<stem>-db/` as the db dir,
    ///   and flag the JSON file for one-time migration.
    /// - Otherwise treat the path as the db directory itself.
    fn resolve_paths(input: &Path) -> (PathBuf, Option<PathBuf>) {
        let path_str = input.to_string_lossy();
        if path_str.ends_with(LEGACY_JSON_SUFFIX) {
            let stem = input.file_stem().unwrap_or_default().to_string_lossy();
            let parent = input.parent().unwrap_or_else(|| Path::new("."));
            let db_dir = parent.join(format!("{stem}-db"));
            (db_dir, Some(input.to_path_buf()))
        } else {
            (input.to_path_buf(), None)
        }
    }

    /// Read one entry by canonical path.
    pub fn get(&self, path: &str) -> Result<Option<MetadataIndexEntry>, FFRError> {
        let rtxn = self
            .env
            .read_txn()
            .map_err(|e| FFRError::Internal(format!("lmdb read_txn: {e}")))?;
        let got = self
            .db
            .get(&rtxn, path)
            .map_err(|e| FFRError::Internal(format!("lmdb get: {e}")))?;
        Ok(got)
    }

    /// Upsert (insert or replace) an entry keyed by its `path` field.
    pub fn upsert(&self, entry: &MetadataIndexEntry) -> Result<(), FFRError> {
        let mut wtxn = self
            .env
            .write_txn()
            .map_err(|e| FFRError::Internal(format!("lmdb write_txn: {e}")))?;
        self.db
            .put(&mut wtxn, &entry.path, entry)
            .map_err(|e| FFRError::Internal(format!("lmdb put: {e}")))?;
        wtxn.commit()
            .map_err(|e| FFRError::Internal(format!("lmdb commit: {e}")))?;
        Ok(())
    }

    /// Remove an entry by path. Returns `true` if an entry was removed.
    pub fn remove(&self, path: &str) -> Result<bool, FFRError> {
        let mut wtxn = self
            .env
            .write_txn()
            .map_err(|e| FFRError::Internal(format!("lmdb write_txn: {e}")))?;
        let removed = self
            .db
            .delete(&mut wtxn, path)
            .map_err(|e| FFRError::Internal(format!("lmdb delete: {e}")))?;
        wtxn.commit()
            .map_err(|e| FFRError::Internal(format!("lmdb commit: {e}")))?;
        Ok(removed)
    }

    /// Delete all entries.
    pub fn clear(&self) -> Result<(), FFRError> {
        let mut wtxn = self
            .env
            .write_txn()
            .map_err(|e| FFRError::Internal(format!("lmdb write_txn: {e}")))?;
        self.db
            .clear(&mut wtxn)
            .map_err(|e| FFRError::Internal(format!("lmdb clear: {e}")))?;
        wtxn.commit()
            .map_err(|e| FFRError::Internal(format!("lmdb commit: {e}")))?;
        Ok(())
    }

    /// Count entries.
    pub fn count(&self) -> Result<u64, FFRError> {
        let rtxn = self
            .env
            .read_txn()
            .map_err(|e| FFRError::Internal(format!("lmdb read_txn: {e}")))?;
        self.db
            .len(&rtxn)
            .map_err(|e| FFRError::Internal(format!("lmdb len: {e}")))
    }

    /// Directory path of this LMDB env.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Disk size of the LMDB env.
    pub fn disk_size(&self) -> Result<u64, FFRError> {
        self.env
            .real_disk_size()
            .map_err(|e| FFRError::Internal(format!("lmdb disk_size: {e}")))
    }

    // ─── Semantic chunk cache ─────────────────────────────────────────

    pub fn get_semantic(&self, path: &str) -> Result<Option<SemanticRecord>, FFRError> {
        let rtxn = self
            .env
            .read_txn()
            .map_err(|e| FFRError::Internal(format!("lmdb read_txn: {e}")))?;
        self.semantic
            .get(&rtxn, path)
            .map_err(|e| FFRError::Internal(format!("lmdb semantic get: {e}")))
    }

    pub fn upsert_semantic(&self, path: &str, record: &SemanticRecord) -> Result<(), FFRError> {
        let mut wtxn = self
            .env
            .write_txn()
            .map_err(|e| FFRError::Internal(format!("lmdb write_txn: {e}")))?;
        self.semantic
            .put(&mut wtxn, path, record)
            .map_err(|e| FFRError::Internal(format!("lmdb semantic put: {e}")))?;
        wtxn.commit()
            .map_err(|e| FFRError::Internal(format!("lmdb commit: {e}")))?;
        Ok(())
    }

    pub fn remove_semantic(&self, path: &str) -> Result<bool, FFRError> {
        let mut wtxn = self
            .env
            .write_txn()
            .map_err(|e| FFRError::Internal(format!("lmdb write_txn: {e}")))?;
        let removed = self
            .semantic
            .delete(&mut wtxn, path)
            .map_err(|e| FFRError::Internal(format!("lmdb semantic delete: {e}")))?;
        wtxn.commit()
            .map_err(|e| FFRError::Internal(format!("lmdb commit: {e}")))?;
        Ok(removed)
    }

    pub fn clear_semantic(&self) -> Result<(), FFRError> {
        let mut wtxn = self
            .env
            .write_txn()
            .map_err(|e| FFRError::Internal(format!("lmdb write_txn: {e}")))?;
        self.semantic
            .clear(&mut wtxn)
            .map_err(|e| FFRError::Internal(format!("lmdb semantic clear: {e}")))?;
        wtxn.commit()
            .map_err(|e| FFRError::Internal(format!("lmdb commit: {e}")))?;
        Ok(())
    }

    /// Import all entries from a legacy JSON file. Returns count imported.
    pub fn migrate_from_json(&self, json_path: &Path) -> Result<usize, FFRError> {
        let data = fs::read_to_string(json_path)?;
        if data.trim().is_empty() {
            return Ok(0);
        }
        let index: MetadataIndex = serde_json::from_str(&data)?;
        let mut count = 0usize;
        let mut wtxn = self
            .env
            .write_txn()
            .map_err(|e| FFRError::Internal(format!("lmdb write_txn: {e}")))?;
        for entry in &index.entries {
            self.db
                .put(&mut wtxn, &entry.path, entry)
                .map_err(|e| FFRError::Internal(format!("lmdb put: {e}")))?;
            count += 1;
        }
        wtxn.commit()
            .map_err(|e| FFRError::Internal(format!("lmdb commit: {e}")))?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_entry(path: &str, size: u64) -> MetadataIndexEntry {
        MetadataIndexEntry {
            path: path.to_string(),
            size,
            mtime: 1700000000,
            revision: format!("{size}:1700000000"),
            binary: false,
            encoding: Some("utf-8".to_string()),
            line_count: Some(10),
            line_index_ready: false,
            last_validated: 1700000000,
        }
    }

    #[test]
    fn open_and_upsert_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let db = MetadataDb::open(tmp.path()).unwrap();
        let entry = sample_entry("/a.rs", 100);
        db.upsert(&entry).unwrap();
        let got = db.get("/a.rs").unwrap().unwrap();
        assert_eq!(got.size, 100);
    }

    #[test]
    fn upsert_replaces() {
        let tmp = TempDir::new().unwrap();
        let db = MetadataDb::open(tmp.path()).unwrap();
        db.upsert(&sample_entry("/a.rs", 100)).unwrap();
        db.upsert(&sample_entry("/a.rs", 200)).unwrap();
        let got = db.get("/a.rs").unwrap().unwrap();
        assert_eq!(got.size, 200);
        assert_eq!(db.count().unwrap(), 1);
    }

    #[test]
    fn remove_works() {
        let tmp = TempDir::new().unwrap();
        let db = MetadataDb::open(tmp.path()).unwrap();
        db.upsert(&sample_entry("/a.rs", 100)).unwrap();
        assert!(db.remove("/a.rs").unwrap());
        assert_eq!(db.count().unwrap(), 0);
        assert!(db.get("/a.rs").unwrap().is_none());
    }

    #[test]
    fn clear_empties_db() {
        let tmp = TempDir::new().unwrap();
        let db = MetadataDb::open(tmp.path()).unwrap();
        db.upsert(&sample_entry("/a.rs", 100)).unwrap();
        db.upsert(&sample_entry("/b.rs", 200)).unwrap();
        assert_eq!(db.count().unwrap(), 2);
        db.clear().unwrap();
        assert_eq!(db.count().unwrap(), 0);
    }

    #[test]
    fn persists_across_reopens() {
        let tmp = TempDir::new().unwrap();
        {
            let db = MetadataDb::open(tmp.path()).unwrap();
            db.upsert(&sample_entry("/persist.rs", 42)).unwrap();
        }
        let db = MetadataDb::open(tmp.path()).unwrap();
        let got = db.get("/persist.rs").unwrap().unwrap();
        assert_eq!(got.size, 42);
    }

    #[test]
    fn migrates_legacy_json() {
        let tmp = TempDir::new().unwrap();
        let json_path = tmp.path().join("metadata_cache.json");

        let mut index = MetadataIndex::default();
        index.entries.push(sample_entry("/legacy_a.rs", 10));
        index.entries.push(sample_entry("/legacy_b.rs", 20));
        fs::write(&json_path, serde_json::to_string(&index).unwrap()).unwrap();

        let db = MetadataDb::open(&json_path).unwrap();
        assert_eq!(db.count().unwrap(), 2);
        assert_eq!(db.get("/legacy_a.rs").unwrap().unwrap().size, 10);
        assert_eq!(db.get("/legacy_b.rs").unwrap().unwrap().size, 20);

        // JSON should be deleted after migration
        assert!(!json_path.exists());

        // sibling db dir should exist
        let db_dir = tmp.path().join("metadata_cache-db");
        assert!(db_dir.is_dir());
    }

    #[test]
    fn resolve_paths_json_suffix() {
        let (dir, legacy) = MetadataDb::resolve_paths(Path::new("/tmp/foo/metadata.json"));
        assert_eq!(dir, PathBuf::from("/tmp/foo/metadata-db"));
        assert_eq!(legacy, Some(PathBuf::from("/tmp/foo/metadata.json")));
    }

    #[test]
    fn resolve_paths_directory() {
        let (dir, legacy) = MetadataDb::resolve_paths(Path::new("/tmp/foo/db"));
        assert_eq!(dir, PathBuf::from("/tmp/foo/db"));
        assert_eq!(legacy, None);
    }
}
