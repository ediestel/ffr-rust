//! Image metadata extraction: dimensions + format + optional EXIF.

use std::fs::File;
use std::io::BufReader;

use super::{SpecializedContent, SpecializedEntry, SpecializedKind};
use crate::errors::FFRError;

pub fn extract(path: &str) -> Result<SpecializedContent, FFRError> {
    let reader = image::ImageReader::open(path)
        .map_err(|e| FFRError::IOError(format!("open image {path}: {e}")))?
        .with_guessed_format()
        .map_err(|e| FFRError::IOError(format!("guess format {path}: {e}")))?;

    let format = reader
        .format()
        .map(|f| format!("{:?}", f).to_lowercase())
        .unwrap_or_else(|| "unknown".to_string());

    let dims = reader
        .into_dimensions()
        .map_err(|e| FFRError::Internal(format!("image dimensions {path}: {e}")))?;

    let mut metadata = vec![
        ("format".to_string(), format.clone()),
        ("width".to_string(), dims.0.to_string()),
        ("height".to_string(), dims.1.to_string()),
    ];

    // Best-effort EXIF read. Non-fatal.
    let mut entries: Vec<SpecializedEntry> = Vec::new();
    if let Ok(file) = File::open(path) {
        let mut bufreader = BufReader::new(&file);
        if let Ok(exif) = exif::Reader::new().read_from_container(&mut bufreader) {
            for field in exif.fields() {
                metadata.push((
                    format!("exif:{}", field.tag),
                    field.display_value().with_unit(&exif).to_string(),
                ));
            }
            entries.push(SpecializedEntry {
                name: "[exif fields available]".to_string(),
                size: None,
                is_dir: false,
            });
        }
    }

    let summary = format!("[image] {path} — {format} {}x{}", dims.0, dims.1);

    Ok(SpecializedContent {
        kind: SpecializedKind::Image,
        summary,
        text: None,
        entries,
        metadata,
    })
}
