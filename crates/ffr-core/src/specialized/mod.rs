//! Specialized handlers for non-text file kinds: PDF, image, archive.
//!
//! Each handler returns a normalized [`SpecializedContent`] struct. Handlers
//! are feature-gated (`pdf`, `image-specialized`, `archive`) so downstream
//! consumers can opt out of heavy deps.

use serde::{Deserialize, Serialize};

use crate::errors::FFRError;

#[cfg(feature = "archive")]
pub mod archive;
#[cfg(feature = "image-specialized")]
pub mod image;
#[cfg(feature = "pdf")]
pub mod pdf;

/// Kind of specialized content produced by a handler.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SpecializedKind {
    Pdf,
    Image,
    Archive,
}

/// Entry in a listing (used by archive handlers and multi-item handlers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecializedEntry {
    pub name: String,
    pub size: Option<u64>,
    pub is_dir: bool,
}

/// Normalized output of any specialized handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecializedContent {
    pub kind: SpecializedKind,
    /// One-line summary suitable for a buffer header.
    pub summary: String,
    /// Free-form text (e.g. PDF extracted text, image EXIF dump, archive listing).
    pub text: Option<String>,
    /// Structured entries (e.g. archive contents, PDF page list).
    pub entries: Vec<SpecializedEntry>,
    /// Additional metadata (flat key/value pairs).
    pub metadata: Vec<(String, String)>,
}

impl SpecializedContent {
    pub fn simple(kind: SpecializedKind, summary: impl Into<String>) -> Self {
        Self {
            kind,
            summary: summary.into(),
            text: None,
            entries: Vec::new(),
            metadata: Vec::new(),
        }
    }
}

/// Dispatch an arbitrary path to the right specialized handler based on its
/// extension. Returns `NotFound` / `UnsupportedEncoding` for unknown kinds.
pub fn extract_specialized(path: &str) -> Result<SpecializedContent, FFRError> {
    let lower = path.to_ascii_lowercase();

    #[cfg(feature = "pdf")]
    if lower.ends_with(".pdf") {
        return pdf::extract(path);
    }

    #[cfg(feature = "image-specialized")]
    if is_image_ext(&lower) {
        return image::extract(path);
    }

    #[cfg(feature = "archive")]
    if is_archive_ext(&lower) {
        return archive::extract(path);
    }

    Err(FFRError::UnsupportedEncoding(format!(
        "no specialized handler for {path}"
    )))
}

#[cfg(feature = "image-specialized")]
fn is_image_ext(path: &str) -> bool {
    matches!(
        path.rsplit('.').next(),
        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("webp") | Some("bmp") | Some("ico")
    )
}

#[cfg(feature = "archive")]
fn is_archive_ext(path: &str) -> bool {
    path.ends_with(".zip")
        || path.ends_with(".tar")
        || path.ends_with(".tar.gz")
        || path.ends_with(".tgz")
}
