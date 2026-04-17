//! PDF text extraction via `pdf-extract`. Not all PDFs yield clean text;
//! encrypted or image-only PDFs return an empty string with a note.

use super::{SpecializedContent, SpecializedKind};
use crate::errors::FFRError;

pub fn extract(path: &str) -> Result<SpecializedContent, FFRError> {
    let path_buf = std::path::PathBuf::from(path);
    let text = pdf_extract::extract_text(&path_buf).map_err(|e| {
        FFRError::Internal(format!("pdf extract failed for {path}: {e}"))
    })?;

    let char_count = text.chars().count();
    let summary = if char_count == 0 {
        format!("[pdf] {path} — no extractable text (image-only or encrypted?)")
    } else {
        format!("[pdf] {path} — {} chars extracted", char_count)
    };

    let metadata = vec![
        ("extracted_chars".to_string(), char_count.to_string()),
        ("extraction_engine".to_string(), "pdf-extract".to_string()),
    ];

    Ok(SpecializedContent {
        kind: SpecializedKind::Pdf,
        summary,
        text: Some(text),
        entries: Vec::new(),
        metadata,
    })
}
