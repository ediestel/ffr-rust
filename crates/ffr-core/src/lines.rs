use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::errors::FFRError;
use crate::types::{BuildLineIndexResult, ReadLinesResult};

#[derive(Debug, Clone)]
pub struct LineIndex {
    pub path: String,
    pub revision: String,
    pub offsets: Vec<u64>,
    pub line_count: usize,
}

pub fn build_line_index(path: &str) -> Result<BuildLineIndexResult, FFRError> {
    let path_ref = Path::new(path);
    let index = crate::cache::get_line_index(path_ref)?;

    Ok(BuildLineIndexResult {
        indexed: true,
        line_count: index.line_count as u64,
    })
}

pub fn read_lines(
    path: &str,
    start_line: usize,
    end_line: usize,
) -> Result<ReadLinesResult, FFRError> {
    if start_line == 0 {
        return Err(FFRError::InvalidRange(
            "start_line must be >= 1".to_string(),
        ));
    }

    if end_line < start_line {
        return Err(FFRError::InvalidRange(
            "end_line must be >= start_line".to_string(),
        ));
    }

    let path_ref = Path::new(path);
    let index = crate::cache::get_line_index(path_ref)?;

    if index.line_count == 0 {
        return Ok(ReadLinesResult {
            start_line,
            end_line,
            actual_end_line: 0,
            eof: true,
            lines: Vec::new(),
        });
    }

    if start_line > index.line_count {
        return Ok(ReadLinesResult {
            start_line,
            end_line,
            actual_end_line: index.line_count,
            eof: true,
            lines: Vec::new(),
        });
    }

    let actual_end_line = std::cmp::min(end_line, index.line_count);
    let eof = actual_end_line >= index.line_count;

    let (byte_start, byte_end) = line_to_byte_range(&index, start_line, actual_end_line)?;
    let lines =
        extract_lines_from_range(path_ref, byte_start, byte_end, start_line, actual_end_line)?;

    Ok(ReadLinesResult {
        start_line,
        end_line,
        actual_end_line,
        eof,
        lines,
    })
}

/// Build a line index by streaming through the file in chunks via BufReader.
/// Never loads the entire file into memory at once.
pub fn construct_line_index(path: &Path) -> Result<LineIndex, FFRError> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let size = metadata.len();
    let mtime = metadata
        .modified()
        .map_err(|e| FFRError::IOError(e.to_string()))?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| FFRError::IOError(e.to_string()))?
        .as_secs();

    let revision = format!("{size}:{mtime}");

    if size == 0 {
        return Ok(LineIndex {
            path: path.to_string_lossy().to_string(),
            revision,
            offsets: Vec::new(),
            line_count: 0,
        });
    }

    let mut offsets = Vec::new();
    offsets.push(0u64);

    let mut reader = BufReader::with_capacity(8192, file);
    let mut buf = [0u8; 8192];
    let mut pos: u64 = 0;

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        for i in 0..n {
            if buf[i] == b'\n' {
                let next = pos + (i as u64) + 1;
                if next < size {
                    offsets.push(next);
                }
            }
        }
        pos += n as u64;
    }

    let line_count = offsets.len();

    Ok(LineIndex {
        path: path.to_string_lossy().to_string(),
        revision,
        offsets,
        line_count,
    })
}

pub fn line_to_byte_range(
    index: &LineIndex,
    start_line: usize,
    end_line: usize,
) -> Result<(u64, u64), FFRError> {
    if start_line == 0 || end_line == 0 {
        return Err(FFRError::InvalidRange(
            "line numbers must be >= 1".to_string(),
        ));
    }

    if end_line < start_line {
        return Err(FFRError::InvalidRange(
            "end_line must be >= start_line".to_string(),
        ));
    }

    if index.line_count == 0 {
        return Ok((0, 0));
    }

    if start_line > index.line_count || end_line > index.line_count {
        return Err(FFRError::InvalidRange(format!(
            "requested lines {}..{} exceed line_count {}",
            start_line, end_line, index.line_count
        )));
    }

    let byte_start = index.offsets[start_line - 1];

    let byte_end = if end_line < index.line_count {
        index.offsets[end_line]
    } else {
        std::fs::metadata(&index.path)?.len()
    };

    Ok((byte_start, byte_end))
}

/// Extract lines from a byte range using seek — never reads the entire file.
pub fn extract_lines_from_range(
    path: &Path,
    byte_start: u64,
    byte_end: u64,
    start_line: usize,
    end_line: usize,
) -> Result<Vec<String>, FFRError> {
    if byte_end < byte_start {
        return Err(FFRError::InvalidRange(
            "byte_end must be >= byte_start".to_string(),
        ));
    }

    if start_line == 0 || end_line == 0 || end_line < start_line {
        return Err(FFRError::InvalidRange("invalid line range".to_string()));
    }

    let length = (byte_end - byte_start) as usize;
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(byte_start))?;

    let mut buf = vec![0u8; length];
    let bytes_read = file.read(&mut buf)?;
    buf.truncate(bytes_read);

    let text = crate::decode::decode_bytes(&buf, Some("utf-8"))?;
    let text = crate::decode::normalize_newlines(&text);

    let wanted_count = end_line - start_line + 1;
    let mut lines = Vec::with_capacity(wanted_count);

    for line in text.split('\n').take(wanted_count) {
        lines.push(line.to_string());
    }

    Ok(lines)
}

/// Binary search for the line number at a given byte offset.
/// Returns 1-based line number.
pub fn byte_offset_to_line(index: &LineIndex, offset: u64) -> usize {
    if index.offsets.is_empty() {
        return 1;
    }
    match index.offsets.binary_search(&offset) {
        Ok(pos) => pos + 1,
        Err(pos) => pos.max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_test_file(name: &str, content: &str) -> String {
        let path = format!("/tmp/ffr_lines_test_{name}.txt");
        let mut f = File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_construct_index_empty() {
        let path = make_test_file("empty", "");
        let index = construct_line_index(Path::new(&path)).unwrap();
        assert_eq!(index.line_count, 0);
        assert!(index.offsets.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_construct_index_simple() {
        let path = make_test_file("simple", "line1\nline2\nline3\n");
        let index = construct_line_index(Path::new(&path)).unwrap();
        assert_eq!(index.line_count, 3);
        assert_eq!(index.offsets[0], 0);
        assert_eq!(index.offsets[1], 6);
        assert_eq!(index.offsets[2], 12);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_first_lines() {
        let path = make_test_file("first", "aaa\nbbb\nccc\nddd\neee\n");
        let result = read_lines(&path, 1, 3).unwrap();
        assert_eq!(result.lines.len(), 3);
        assert_eq!(result.lines[0], "aaa");
        assert_eq!(result.lines[2], "ccc");
        assert!(!result.eof);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_last_lines() {
        let path = make_test_file("last", "aaa\nbbb\nccc\n");
        let result = read_lines(&path, 2, 3).unwrap();
        assert_eq!(result.lines.len(), 2);
        assert_eq!(result.lines[0], "bbb");
        assert_eq!(result.lines[1], "ccc");
        assert!(result.eof);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_out_of_range() {
        let path = make_test_file("oor", "aaa\nbbb\n");
        let result = read_lines(&path, 100, 200).unwrap();
        assert!(result.lines.is_empty());
        assert!(result.eof);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_start_line_zero_error() {
        let path = make_test_file("zero", "aaa\n");
        let result = read_lines(&path, 0, 1);
        assert!(result.is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_byte_offset_to_line() {
        let index = LineIndex {
            path: "test".to_string(),
            revision: "0:0".to_string(),
            offsets: vec![0, 10, 20, 30],
            line_count: 4,
        };
        assert_eq!(byte_offset_to_line(&index, 0), 1);
        assert_eq!(byte_offset_to_line(&index, 10), 2);
        assert_eq!(byte_offset_to_line(&index, 15), 2);
        assert_eq!(byte_offset_to_line(&index, 30), 4);
    }

    #[test]
    fn test_middle_lines_in_100k_line_file() {
        let content: String = (0..100_000).map(|i| format!("line {i:06}\n")).collect();
        let path = make_test_file("100k", &content);

        let result = read_lines(&path, 50_000, 50_100).unwrap();
        assert_eq!(result.lines.len(), 101);
        assert_eq!(result.lines[0], "line 049999");
        assert_eq!(result.lines[100], "line 050099");
        assert!(!result.eof);

        let _ = std::fs::remove_file(&path);
    }
}
