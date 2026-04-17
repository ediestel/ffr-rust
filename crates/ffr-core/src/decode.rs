//! Multi-encoding text decoding via `encoding_rs`.
//!
//! Detection order:
//!   1. BOM sniff (UTF-8 / UTF-16 LE / UTF-16 BE)
//!   2. Explicit encoding name supplied by the caller
//!   3. Fallback list (provided by config; defaults to UTF-8 → windows-1252 → latin1)
//!
//! If the selected encoding produces replacement characters (decode errors),
//! `DecodedText::had_errors` is set so the caller may warn the user.

use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, UTF_8};
use serde::{Deserialize, Serialize};

use crate::errors::FFRError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedText {
    pub encoding: String,
    pub text: String,
    pub had_errors: bool,
}

/// Decode bytes using a specified encoding name, or auto-detect via BOM.
/// The `fallback_order` list is tried (in order) when `encoding` is None and
/// no BOM is found.
pub fn decode_bytes_with(
    bytes: &[u8],
    encoding: Option<&str>,
    fallback_order: &[&str],
) -> Result<DecodedText, FFRError> {
    // 1. BOM sniff always wins.
    if let Some((enc, stripped)) = sniff_bom(bytes) {
        return Ok(run_decode(enc, stripped));
    }

    // 2. Explicit encoding.
    if let Some(name) = encoding {
        if let Some(enc) = encoding_for_label(name) {
            return Ok(run_decode(enc, bytes));
        }
        return Err(FFRError::UnsupportedEncoding(format!(
            "unknown encoding: {name}"
        )));
    }

    // 3. Fallback list.
    for name in fallback_order {
        if let Some(enc) = encoding_for_label(name) {
            let decoded = run_decode(enc, bytes);
            if !decoded.had_errors {
                return Ok(decoded);
            }
        }
    }

    // Last resort: UTF-8 lossy.
    Ok(run_decode(UTF_8, bytes))
}

/// Legacy entry point used by readers that don't care about the fallback list.
/// Equivalent to `decode_bytes_with(bytes, encoding, &["utf-8"])`.
pub fn decode_bytes(bytes: &[u8], encoding: Option<&str>) -> Result<String, FFRError> {
    Ok(decode_bytes_with(bytes, encoding, &["utf-8"])?.text)
}

/// Normalize line endings to LF.
pub fn normalize_newlines(input: &str) -> String {
    input.replace("\r\n", "\n").replace('\r', "\n")
}

fn sniff_bom(bytes: &[u8]) -> Option<(&'static Encoding, &[u8])> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Some((UTF_8, &bytes[3..]));
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return Some((UTF_16LE, &bytes[2..]));
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return Some((UTF_16BE, &bytes[2..]));
    }
    None
}

fn encoding_for_label(name: &str) -> Option<&'static Encoding> {
    let norm = name.trim().to_ascii_lowercase();
    // encoding_rs uses IANA names; accept a few common aliases too.
    let label = match norm.as_str() {
        "utf8" => "utf-8",
        "utf16le" | "utf-16-le" => "utf-16le",
        "utf16be" | "utf-16-be" => "utf-16be",
        "latin1" | "iso-8859-1" => "iso-8859-1",
        "sjis" | "shift_jis" => "shift_jis",
        other => other,
    };
    Encoding::for_label(label.as_bytes())
}

fn run_decode(enc: &'static Encoding, bytes: &[u8]) -> DecodedText {
    let (cow, _used_enc, had_errors) = enc.decode(bytes);
    DecodedText {
        encoding: enc.name().to_lowercase(),
        text: cow.into_owned(),
        had_errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_roundtrip() {
        let d = decode_bytes_with("hello".as_bytes(), Some("utf-8"), &[]).unwrap();
        assert_eq!(d.encoding, "utf-8");
        assert_eq!(d.text, "hello");
        assert!(!d.had_errors);
    }

    #[test]
    fn utf8_bom_stripped() {
        let bytes = [0xEF, 0xBB, 0xBF, b'h', b'i'];
        let d = decode_bytes_with(&bytes, None, &[]).unwrap();
        assert_eq!(d.text, "hi");
    }

    #[test]
    fn utf16le_bom() {
        // "hi" in UTF-16 LE with BOM
        let bytes: &[u8] = &[0xFF, 0xFE, b'h', 0, b'i', 0];
        let d = decode_bytes_with(bytes, None, &[]).unwrap();
        assert_eq!(d.text, "hi");
        assert_eq!(d.encoding, "utf-16le");
    }

    #[test]
    fn latin1_via_fallback() {
        // Byte 0xE9 is é in latin-1, invalid in UTF-8.
        let bytes: &[u8] = &[0xE9, b'h', b'i'];
        let d = decode_bytes_with(bytes, None, &["utf-8", "latin1"]).unwrap();
        assert!(d.text.contains('é') || d.text.starts_with('\u{E9}'));
    }

    #[test]
    fn unknown_label_errors() {
        let r = decode_bytes_with("x".as_bytes(), Some("bogus-enc"), &[]);
        assert!(r.is_err());
    }

    #[test]
    fn legacy_entry_point() {
        let s = decode_bytes("hello".as_bytes(), Some("utf-8")).unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn normalize_line_endings() {
        assert_eq!(normalize_newlines("a\r\nb\rc\n"), "a\nb\nc\n");
    }
}
