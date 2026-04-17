use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct StatPathResult {
    pub exists: bool,
    pub is_file: bool,
    pub size: u64,
    pub mtime: u64,
    pub readonly: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClassifyPathResult {
    pub kind: String,
    pub binary: bool,
    pub encoding: Option<String>,
    pub line_ending: Option<String>,
    pub estimated_lines: Option<u64>,
    pub too_large_for_full_open: bool,
    pub preview_allowed: bool,
    pub reason: Option<String>,
    pub likely_filetype: Option<String>,
    pub minified: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadBytesResult {
    pub bytes_read: usize,
    pub eof: bool,
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadLinesResult {
    pub start_line: usize,
    pub end_line: usize,
    pub actual_end_line: usize,
    pub eof: bool,
    pub lines: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildLineIndexResult {
    pub indexed: bool,
    pub line_count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadChunkResult {
    pub chunk_id: u64,
    pub byte_start: u64,
    pub byte_end: u64,
    pub start_line: usize,
    pub end_line: usize,
    pub eof: bool,
    pub text: String,
}
