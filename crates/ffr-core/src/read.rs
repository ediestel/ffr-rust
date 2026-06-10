use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::decode;
use crate::errors::FFRError;
use crate::lines;
use crate::types::{ReadBytesResult, ReadChunkResult};

pub fn read_bytes(path: &str, offset: u64, length: usize) -> Result<ReadBytesResult, FFRError> {
    let path_ref = Path::new(path);

    let mut file = File::open(path_ref)?;
    let file_len = file.metadata()?.len();

    if offset > file_len {
        return Err(FFRError::InvalidRange(format!(
            "offset {} beyond file length {}",
            offset, file_len
        )));
    }

    file.seek(SeekFrom::Start(offset))?;

    let mut buf = vec![0u8; length];
    let bytes_read = file.read(&mut buf)?;
    buf.truncate(bytes_read);

    let eof = offset + (bytes_read as u64) >= file_len;
    let data = decode::decode_bytes(&buf, Some("utf-8"))?;

    Ok(ReadBytesResult {
        bytes_read,
        eof,
        data,
    })
}

/// Deterministic chunk reader with accurate line numbers from the line index.
pub fn read_chunk(
    path: &str,
    chunk_id: u64,
    chunk_bytes: usize,
) -> Result<ReadChunkResult, FFRError> {
    let path_ref = Path::new(path);

    let mut file = File::open(path_ref)?;
    let file_len = file.metadata()?.len();

    let byte_start = chunk_id * (chunk_bytes as u64);

    if byte_start > file_len {
        return Err(FFRError::InvalidRange(format!(
            "chunk {} starts beyond file",
            chunk_id
        )));
    }

    let byte_end = std::cmp::min(byte_start + (chunk_bytes as u64), file_len);
    let length = (byte_end - byte_start) as usize;

    file.seek(SeekFrom::Start(byte_start))?;

    let mut buf = vec![0u8; length];
    let bytes_read = file.read(&mut buf)?;
    buf.truncate(bytes_read);

    let eof = byte_end >= file_len;
    let text = decode::normalize_newlines(&decode::decode_bytes(&buf, Some("utf-8"))?);

    // Use line index for accurate line numbers when available.
    // Falls back to counting lines in the chunk text.
    let (start_line, end_line) = resolve_chunk_lines(path_ref, byte_start, &text);

    Ok(ReadChunkResult {
        chunk_id,
        byte_start,
        byte_end,
        start_line,
        end_line,
        eof,
        text,
    })
}

/// Low-level range read (shared utility)
pub fn read_range(path: &Path, offset: u64, length: usize) -> Result<Vec<u8>, FFRError> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;

    let mut buf = vec![0u8; length];
    let bytes_read = file.read(&mut buf)?;
    buf.truncate(bytes_read);

    Ok(buf)
}

/// Resolve chunk line numbers: use the cached line index when available,
/// else stream-count newlines up to the chunk start so later chunks still
/// report correct line numbers.
fn resolve_chunk_lines(path: &Path, byte_start: u64, text: &str) -> (usize, usize) {
    let start_line = if let Ok(index) = crate::cache::get_line_index(path) {
        lines::byte_offset_to_line(&index, byte_start)
    } else {
        lines::count_newlines_before(path, byte_start)
            .map(|newlines| newlines as usize + 1)
            .unwrap_or(1)
    };

    let line_count = text.lines().count().max(1);
    (start_line, start_line + line_count - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_test_file(name: &str, content: &str) -> String {
        let path = format!("/tmp/ffr_read_test_{name}.txt");
        let mut f = File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_read_bytes_basic() {
        let path = make_test_file("bytes_basic", "hello world");
        let result = read_bytes(&path, 0, 5).unwrap();
        assert_eq!(result.data, "hello");
        assert_eq!(result.bytes_read, 5);
        assert!(!result.eof);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_bytes_eof() {
        let path = make_test_file("bytes_eof", "hello");
        let result = read_bytes(&path, 0, 100).unwrap();
        assert_eq!(result.data, "hello");
        assert!(result.eof);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_bytes_offset_beyond() {
        let path = make_test_file("bytes_beyond", "hello");
        let result = read_bytes(&path, 999, 1);
        assert!(result.is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_chunk_deterministic() {
        let content = "line1\nline2\nline3\nline4\nline5\n";
        let path = make_test_file("chunk_det", content);
        let chunk_size = 12;

        let c0 = read_chunk(&path, 0, chunk_size).unwrap();
        assert_eq!(c0.chunk_id, 0);
        assert_eq!(c0.byte_start, 0);
        assert_eq!(c0.byte_end, 12);
        assert!(!c0.eof);

        // Same chunk_id yields same byte range
        let c0_again = read_chunk(&path, 0, chunk_size).unwrap();
        assert_eq!(c0.byte_start, c0_again.byte_start);
        assert_eq!(c0.byte_end, c0_again.byte_end);
        assert_eq!(c0.text, c0_again.text);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_chunk_eof() {
        let path = make_test_file("chunk_eof", "short");
        let result = read_chunk(&path, 0, 65536).unwrap();
        assert!(result.eof);
        assert_eq!(result.text, "short");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_chunk_beyond() {
        let path = make_test_file("chunk_bey", "hello");
        let result = read_chunk(&path, 100, 64);
        assert!(result.is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_chunk_next_prev_transitions() {
        let content = (0..100)
            .map(|i| format!("line {i:04}"))
            .collect::<Vec<_>>()
            .join("\n");
        let path = make_test_file("chunk_nav", &content);
        let chunk_size = 50;

        let c0 = read_chunk(&path, 0, chunk_size).unwrap();
        assert_eq!(c0.chunk_id, 0);
        assert!(!c0.eof);

        let c1 = read_chunk(&path, 1, chunk_size).unwrap();
        assert_eq!(c1.chunk_id, 1);
        assert_eq!(c1.byte_start, chunk_size as u64);

        let _ = std::fs::remove_file(&path);
    }
}
