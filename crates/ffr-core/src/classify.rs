use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::errors::FFRError;
use crate::types::ClassifyPathResult;
use crate::stat;

/// Classify a path using the provided threshold parameters (from runtime config).
pub fn classify_path(
    path: &str,
    sniff_bytes: usize,
    full_open_max_bytes: u64,
    minified_line_length_threshold: usize,
) -> Result<ClassifyPathResult, FFRError> {
    let path_ref = Path::new(path);

    if !stat::path_exists(path_ref) {
        return Err(FFRError::NotFound(format!("path not found: {path}")));
    }

    if !stat::is_regular_file(path_ref)? {
        return Ok(ClassifyPathResult {
            kind: "unknown".to_string(),
            binary: false,
            encoding: None,
            line_ending: None,
            estimated_lines: None,
            too_large_for_full_open: false,
            preview_allowed: false,
            reason: Some("path is not a regular file".to_string()),
            likely_filetype: None,
            minified: None,
        });
    }

    let size = stat::file_size(path_ref)?;
    let binary = sniff_binary(path_ref, sniff_bytes)?;

    let mut kind = detect_kind(path_ref);
    let likely_filetype = likely_filetype(path_ref);

    if binary {
        if kind == "unknown" {
            kind = "binary".to_string();
        }

        let preview_allowed = kind == "pdf";
        let reason = if preview_allowed {
            Some("specialized handler required".to_string())
        } else {
            Some("binary file".to_string())
        };

        return Ok(ClassifyPathResult {
            kind,
            binary: true,
            encoding: None,
            line_ending: None,
            estimated_lines: None,
            too_large_for_full_open: size > full_open_max_bytes,
            preview_allowed,
            reason,
            likely_filetype,
            minified: None,
        });
    }

    let encoding = detect_encoding(path_ref, sniff_bytes)?;
    let line_ending = detect_line_ending(path_ref, sniff_bytes)?;
    let estimated_lines = estimate_lines(path_ref, sniff_bytes)?;
    let minified = is_minified(path_ref, sniff_bytes, minified_line_length_threshold)?;

    if minified == Some(true) {
        kind = "minified".to_string();
    }

    let too_large_for_full_open = size > full_open_max_bytes;
    let preview_allowed = true;

    // Upsert into persistent metadata cache
    let mtime = stat::file_mtime_unix(path_ref).unwrap_or(0);
    let _ = crate::cache::upsert_metadata_entry(crate::index::MetadataIndexEntry {
        path: path.to_string(),
        size,
        mtime,
        revision: crate::index::compute_revision(size, mtime),
        binary: false,
        encoding: encoding.clone(),
        line_count: estimated_lines,
        line_index_ready: false,
        last_validated: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    });

    Ok(ClassifyPathResult {
        kind,
        binary: false,
        encoding,
        line_ending,
        estimated_lines,
        too_large_for_full_open,
        preview_allowed,
        reason: None,
        likely_filetype,
        minified,
    })
}

pub fn detect_kind(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());

    match ext.as_deref() {
        Some("txt")
        | Some("md")
        | Some("rs")
        | Some("lua")
        | Some("py")
        | Some("js")
        | Some("ts")
        | Some("tsx")
        | Some("jsx")
        | Some("c")
        | Some("h")
        | Some("cpp")
        | Some("hpp")
        | Some("go")
        | Some("java")
        | Some("sh")
        | Some("zsh")
        | Some("toml")
        | Some("yaml")
        | Some("yml")
        | Some("ini")
        | Some("conf")
        | Some("log")
        | Some("csv")
        | Some("sql")
        | Some("html")
        | Some("css")
        | Some("vim") => "text".to_string(),

        Some("json") => "json".to_string(),

        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("webp") | Some("bmp")
        | Some("svg") | Some("ico") => "image".to_string(),

        Some("pdf") => "pdf".to_string(),

        Some("zip") | Some("tar") | Some("gz") | Some("xz") | Some("bz2") | Some("7z")
        | Some("rar") => "archive".to_string(),

        Some("so") | Some("dll") | Some("dylib") | Some("exe") | Some("bin") | Some("o")
        | Some("a") | Some("class") | Some("jar") => "binary".to_string(),

        _ => "unknown".to_string(),
    }
}

pub fn sniff_binary(path: &Path, sniff_bytes: usize) -> Result<bool, FFRError> {
    let sample = read_prefix(path, sniff_bytes)?;

    if sample.is_empty() {
        return Ok(false);
    }

    if sample.contains(&0) {
        return Ok(true);
    }

    let mut suspicious = 0usize;

    for &b in &sample {
        let is_allowed =
            matches!(b, 0x09 | 0x0A | 0x0D) || (0x20..=0x7E).contains(&b) || b >= 0x80;

        if !is_allowed {
            suspicious += 1;
        }
    }

    Ok((suspicious as f64) / (sample.len() as f64) > 0.30)
}

pub fn detect_encoding(path: &Path, sniff_bytes: usize) -> Result<Option<String>, FFRError> {
    let sample = read_prefix(path, sniff_bytes)?;

    if sample.is_empty() {
        return Ok(Some("utf-8".to_string()));
    }

    if std::str::from_utf8(&sample).is_ok() {
        return Ok(Some("utf-8".to_string()));
    }

    Ok(None)
}

pub fn detect_line_ending(path: &Path, sniff_bytes: usize) -> Result<Option<String>, FFRError> {
    let sample = read_prefix(path, sniff_bytes)?;

    if sample.windows(2).any(|w| w == b"\r\n") {
        return Ok(Some("crlf".to_string()));
    }

    if sample.contains(&b'\n') {
        return Ok(Some("lf".to_string()));
    }

    if sample.contains(&b'\r') {
        return Ok(Some("cr".to_string()));
    }

    Ok(None)
}

pub fn estimate_lines(path: &Path, sniff_bytes: usize) -> Result<Option<u64>, FFRError> {
    let size = stat::file_size(path)?;
    let sample = read_prefix(path, sniff_bytes)?;

    if sample.is_empty() {
        return Ok(Some(0));
    }

    let newline_count = sample.iter().filter(|&&b| b == b'\n').count() as u64;

    if newline_count == 0 {
        return Ok(Some(1));
    }

    let sample_len = sample.len() as u64;
    if sample_len == 0 {
        return Ok(Some(0));
    }

    let estimated = ((newline_count * size) / sample_len).max(1);
    Ok(Some(estimated))
}

pub fn is_minified(
    path: &Path,
    sniff_bytes: usize,
    threshold: usize,
) -> Result<Option<bool>, FFRError> {
    let sample = read_prefix(path, sniff_bytes)?;

    if sample.is_empty() {
        return Ok(Some(false));
    }

    let text = match std::str::from_utf8(&sample) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };

    let mut longest_line = 0usize;
    let mut line_count = 0usize;

    for line in text.lines() {
        line_count += 1;
        let len = line.len();
        if len > longest_line {
            longest_line = len;
        }
    }

    if line_count == 0 {
        return Ok(Some(false));
    }

    let newline_density = (line_count as f64) / (text.len().max(1) as f64);

    let minified_like =
        longest_line >= threshold || (text.len() > threshold * 2 && newline_density < 0.002);

    Ok(Some(minified_like))
}

pub fn likely_filetype(path: &Path) -> Option<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());

    match ext.as_deref() {
        Some("rs") => Some("rust".to_string()),
        Some("lua") => Some("lua".to_string()),
        Some("py") => Some("python".to_string()),
        Some("js") => Some("javascript".to_string()),
        Some("ts") => Some("typescript".to_string()),
        Some("tsx") => Some("typescriptreact".to_string()),
        Some("jsx") => Some("javascriptreact".to_string()),
        Some("json") => Some("json".to_string()),
        Some("md") => Some("markdown".to_string()),
        Some("toml") => Some("toml".to_string()),
        Some("yaml") | Some("yml") => Some("yaml".to_string()),
        Some("html") => Some("html".to_string()),
        Some("css") => Some("css".to_string()),
        Some("sh") | Some("zsh") => Some("sh".to_string()),
        Some("c") => Some("c".to_string()),
        Some("h") => Some("c".to_string()),
        Some("cpp") | Some("hpp") => Some("cpp".to_string()),
        Some("go") => Some("go".to_string()),
        Some("java") => Some("java".to_string()),
        Some("sql") => Some("sql".to_string()),
        Some("txt") | Some("log") => Some("text".to_string()),
        _ => None,
    }
}

fn read_prefix(path: &Path, max_bytes: usize) -> Result<Vec<u8>, FFRError> {
    let mut file = File::open(path)?;
    let mut buf = vec![0u8; max_bytes];
    let n = file.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_test_file(name: &str, content: &[u8]) -> String {
        let path = format!("/tmp/ffr_classify_test_{name}");
        let mut f = File::create(&path).unwrap();
        f.write_all(content).unwrap();
        path
    }

    #[test]
    fn test_classify_utf8_text() {
        let path = make_test_file("utf8.txt", b"hello\nworld\n");
        let result = classify_path(&path, 4096, 2 * 1024 * 1024, 1000).unwrap();
        assert_eq!(result.kind, "text");
        assert!(!result.binary);
        assert_eq!(result.encoding, Some("utf-8".to_string()));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_classify_binary_with_nul() {
        let mut content = b"hello".to_vec();
        content.push(0);
        content.extend_from_slice(b"world");
        let path = make_test_file("binary.bin", &content);
        let result = classify_path(&path, 4096, 2 * 1024 * 1024, 1000).unwrap();
        assert!(result.binary);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_classify_empty_file() {
        let path = make_test_file("empty.txt", b"");
        let result = classify_path(&path, 4096, 2 * 1024 * 1024, 1000).unwrap();
        assert!(!result.binary);
        assert_eq!(result.estimated_lines, Some(0));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_classify_minified() {
        let long_line = "x".repeat(2000);
        let path = make_test_file("minified.js", long_line.as_bytes());
        let result = classify_path(&path, 4096, 2 * 1024 * 1024, 1000).unwrap();
        assert_eq!(result.kind, "minified");
        assert_eq!(result.minified, Some(true));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_classify_not_found() {
        let result = classify_path("/tmp/ffr_nonexistent_file_xyz", 4096, 2 * 1024 * 1024, 1000);
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_kind_extensions() {
        assert_eq!(detect_kind(Path::new("foo.rs")), "text");
        assert_eq!(detect_kind(Path::new("foo.json")), "json");
        assert_eq!(detect_kind(Path::new("foo.png")), "image");
        assert_eq!(detect_kind(Path::new("foo.pdf")), "pdf");
        assert_eq!(detect_kind(Path::new("foo.zip")), "archive");
        assert_eq!(detect_kind(Path::new("foo.exe")), "binary");
        assert_eq!(detect_kind(Path::new("foo.xyz")), "unknown");
    }

    #[test]
    fn test_classify_utf16_detected_as_binary() {
        // UTF-16 LE BOM + "hi" encoded as UTF-16 LE
        let content: Vec<u8> = vec![0xFF, 0xFE, b'h', 0x00, b'i', 0x00, b'\n', 0x00];
        let path = make_test_file("utf16.txt", &content);
        let result = classify_path(&path, 4096, 2 * 1024 * 1024, 1000).unwrap();
        // NUL bytes from UTF-16 encoding trigger binary detection
        assert!(result.binary);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_classify_huge_text_too_large_for_full_open() {
        // 3MB text file — exceeds 2MB full_open_max_bytes threshold
        let line = "x".repeat(99) + "\n";
        let content = line.repeat(30_000); // ~3MB
        let path = make_test_file("huge.txt", content.as_bytes());
        let result = classify_path(&path, 4096, 2 * 1024 * 1024, 1000).unwrap();
        assert_eq!(result.kind, "text");
        assert!(!result.binary);
        assert!(result.too_large_for_full_open);
        assert!(result.preview_allowed);
        let _ = std::fs::remove_file(&path);
    }
}
