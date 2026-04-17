use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, Write};

pub const PROTOCOL_VERSION: &str = "1";

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    pub params: Value,
    #[serde(default)]
    pub protocol_version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    pub protocol_version: String,
    pub result: Option<Value>,
    pub error: Option<ErrorPayload>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
}

// --- Param structs ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigureParams {
    #[serde(default = "default_chunk_bytes")]
    pub chunk_bytes: usize,
    #[serde(default = "default_full_open_max_bytes")]
    pub full_open_max_bytes: u64,
    #[serde(default = "default_binary_sniff_bytes")]
    pub binary_sniff_bytes: usize,
    #[serde(default = "default_minified_line_length_threshold")]
    pub minified_line_length_threshold: usize,
    pub metadata_cache_path: Option<String>,
}

fn default_chunk_bytes() -> usize {
    64 * 1024
}
fn default_full_open_max_bytes() -> u64 {
    2 * 1024 * 1024
}
fn default_binary_sniff_bytes() -> usize {
    4096
}
fn default_minified_line_length_threshold() -> usize {
    1000
}

impl Default for ConfigureParams {
    fn default() -> Self {
        Self {
            chunk_bytes: default_chunk_bytes(),
            full_open_max_bytes: default_full_open_max_bytes(),
            binary_sniff_bytes: default_binary_sniff_bytes(),
            minified_line_length_threshold: default_minified_line_length_threshold(),
            metadata_cache_path: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigureResult {
    pub ok: bool,
    pub protocol_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatPathParams {
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClassifyPathParams {
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadBytesParams {
    pub path: String,
    pub offset: u64,
    pub length: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadLinesParams {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildLineIndexParams {
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadChunkParams {
    pub path: String,
    pub chunk_id: u64,
    #[serde(default)]
    pub chunk_bytes: Option<usize>,
}

// --- I/O ---

pub fn parse_request(line: &str) -> Result<Request, serde_json::Error> {
    serde_json::from_str(line)
}

pub fn write_response(resp: &Response) -> Result<(), io::Error> {
    let mut stdout = io::stdout().lock();
    let line = serde_json::to_string(resp).expect("response serialization must succeed");
    stdout.write_all(line.as_bytes())?;
    stdout.write_all(b"\n")?;
    stdout.flush()
}

pub fn ok_response<T: Serialize>(id: u64, result: T) -> Response {
    Response {
        id,
        ok: true,
        protocol_version: PROTOCOL_VERSION.to_string(),
        result: Some(serde_json::to_value(result).expect("result serialization must succeed")),
        error: None,
    }
}

pub fn err_response(id: u64, code: &str, message: &str) -> Response {
    Response {
        id,
        ok: false,
        protocol_version: PROTOCOL_VERSION.to_string(),
        result: None,
        error: Some(ErrorPayload {
            code: code.to_string(),
            message: message.to_string(),
        }),
    }
}
