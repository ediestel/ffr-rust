//! Metadata entry types and revision computation.
//!
//! Persistence lives in [`crate::db::MetadataDb`] (LMDB). `MetadataIndex` is
//! retained as a serde-compatible shape used only by the one-time JSON
//! migrator in `db::MetadataDb::migrate_from_json`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataIndexEntry {
    pub path: String,
    pub size: u64,
    pub mtime: u64,
    pub revision: String,
    pub binary: bool,
    pub encoding: Option<String>,
    pub line_count: Option<u64>,
    pub line_index_ready: bool,
    pub last_validated: u64,
}

/// Legacy JSON shape. Only used by the LMDB migrator.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MetadataIndex {
    pub entries: Vec<MetadataIndexEntry>,
}

pub fn compute_revision(size: u64, mtime: u64) -> String {
    format!("{size}:{mtime}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_revision() {
        assert_eq!(compute_revision(1024, 1700000000), "1024:1700000000");
    }

    #[test]
    fn test_entry_serde_roundtrip() {
        let entry = MetadataIndexEntry {
            path: "/a.rs".to_string(),
            size: 100,
            mtime: 1,
            revision: "100:1".to_string(),
            binary: false,
            encoding: Some("utf-8".to_string()),
            line_count: Some(5),
            line_index_ready: false,
            last_validated: 1,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: MetadataIndexEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.path, "/a.rs");
        assert_eq!(back.size, 100);
    }
}
