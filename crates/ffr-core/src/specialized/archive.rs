//! Archive listing for zip / tar / tar.gz.
//!
//! Listings are capped at 500 entries by default; callers can enforce their
//! own limit via config (not threaded through the Rust API — the Lua renderer
//! trims `entries` as needed).

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use super::{SpecializedContent, SpecializedEntry, SpecializedKind};
use crate::errors::FFRError;

const HARD_CAP: usize = 500;

pub fn extract(path: &str) -> Result<SpecializedContent, FFRError> {
    let p = Path::new(path);
    let lower = path.to_ascii_lowercase();

    if lower.ends_with(".zip") {
        list_zip(p)
    } else if lower.ends_with(".tar") {
        list_tar(p, false)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        list_tar(p, true)
    } else {
        Err(FFRError::UnsupportedEncoding(format!(
            "unknown archive extension: {path}"
        )))
    }
}

fn list_zip(path: &Path) -> Result<SpecializedContent, FFRError> {
    let file = File::open(path)
        .map_err(|e| FFRError::IOError(format!("open zip {}: {e}", path.display())))?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file))
        .map_err(|e| FFRError::Internal(format!("read zip {}: {e}", path.display())))?;

    let total = archive.len();
    let mut entries = Vec::with_capacity(total.min(HARD_CAP));
    let mut total_size = 0u64;
    for i in 0..total.min(HARD_CAP) {
        let entry = archive
            .by_index(i)
            .map_err(|e| FFRError::Internal(format!("zip entry {i}: {e}")))?;
        total_size += entry.size();
        entries.push(SpecializedEntry {
            name: entry.name().to_string(),
            size: Some(entry.size()),
            is_dir: entry.is_dir(),
        });
    }

    let summary = format!(
        "[zip] {} — {} entries, {} bytes uncompressed",
        path.display(),
        total,
        total_size
    );

    Ok(SpecializedContent {
        kind: SpecializedKind::Archive,
        summary,
        text: None,
        entries,
        metadata: vec![
            ("format".to_string(), "zip".to_string()),
            ("entry_count".to_string(), total.to_string()),
            ("uncompressed_size".to_string(), total_size.to_string()),
        ],
    })
}

fn list_tar(path: &Path, gzipped: bool) -> Result<SpecializedContent, FFRError> {
    let file = File::open(path)
        .map_err(|e| FFRError::IOError(format!("open tar {}: {e}", path.display())))?;
    let reader: Box<dyn std::io::Read> = if gzipped {
        Box::new(flate2::read::GzDecoder::new(BufReader::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };
    let mut archive = tar::Archive::new(reader);

    let mut entries = Vec::new();
    let mut total_size = 0u64;
    let mut count = 0usize;
    for entry in archive
        .entries()
        .map_err(|e| FFRError::Internal(format!("tar entries: {e}")))?
    {
        let entry = entry.map_err(|e| FFRError::Internal(format!("tar entry: {e}")))?;
        let header = entry.header();
        let name = entry
            .path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "?".to_string());
        let size = header.size().unwrap_or(0);
        let is_dir = header.entry_type().is_dir();
        total_size += size;
        count += 1;
        if entries.len() < HARD_CAP {
            entries.push(SpecializedEntry {
                name,
                size: Some(size),
                is_dir,
            });
        }
    }

    let fmt_name = if gzipped { "tar.gz" } else { "tar" };
    let summary = format!(
        "[{fmt_name}] {} — {} entries, {} bytes uncompressed",
        path.display(),
        count,
        total_size
    );

    Ok(SpecializedContent {
        kind: SpecializedKind::Archive,
        summary,
        text: None,
        entries,
        metadata: vec![
            ("format".to_string(), fmt_name.to_string()),
            ("entry_count".to_string(), count.to_string()),
            ("uncompressed_size".to_string(), total_size.to_string()),
        ],
    })
}
