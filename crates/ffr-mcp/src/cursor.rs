//! Pagination cursor encoding/decoding. Cursors are opaque base16 strings
//! embedding a simple "offset:limit" pair so the server can resume any
//! range-based result set without maintaining server-side state.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cursor {
    pub offset: usize,
    pub limit: usize,
}

impl Cursor {
    pub fn encode(&self) -> String {
        let raw = format!("{}:{}", self.offset, self.limit);
        hex_encode(raw.as_bytes())
    }

    pub fn decode(s: &str) -> Option<Self> {
        let bytes = hex_decode(s)?;
        let text = String::from_utf8(bytes).ok()?;
        let mut parts = text.splitn(2, ':');
        let offset: usize = parts.next()?.parse().ok()?;
        let limit: usize = parts.next()?.parse().ok()?;
        Some(Cursor { offset, limit })
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let chars: Vec<char> = s.chars().collect();
    for chunk in chars.chunks(2) {
        let hex: String = chunk.iter().collect();
        out.push(u8::from_str_radix(&hex, 16).ok()?);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let c = Cursor { offset: 42, limit: 100 };
        let encoded = c.encode();
        let back = Cursor::decode(&encoded).unwrap();
        assert_eq!(back.offset, 42);
        assert_eq!(back.limit, 100);
    }

    #[test]
    fn bad_decode() {
        assert!(Cursor::decode("zz").is_none());
        assert!(Cursor::decode("abc").is_none());
    }
}
