//! C FFI layer for ffr.
//!
//! All functions return a freshly allocated C string owned by the caller —
//! must be freed with [`ffr_free_string`]. The string is UTF-8 JSON: either
//! the serialized result struct, or `{"error": {"code": "...", "message": "..."}}`.
//!
//! # Safety
//! Path arguments must be valid UTF-8 NUL-terminated C strings.

use std::ffi::{CStr, CString, c_char};

use ffr_core::{classify, lines, read, stat};

// ---------------------------------------------------------------------------
// string alloc
// ---------------------------------------------------------------------------

fn to_cstring(json: String) -> *mut c_char {
    CString::new(json).unwrap_or_else(|_| {
        CString::new("{\"error\":{\"code\":\"Internal\",\"message\":\"nul in output\"}}").unwrap()
    })
    .into_raw()
}

fn error_json(code: &str, message: &str) -> *mut c_char {
    let obj = serde_json::json!({
        "error": { "code": code, "message": message }
    });
    to_cstring(obj.to_string())
}

fn cstr_to_str<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(p) }.to_str().ok()
}

/// Free a string returned by any `ffr_c_*` function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffr_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}

// ---------------------------------------------------------------------------
// exported functions
// ---------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffr_c_stat(path: *const c_char) -> *mut c_char {
    let path = match cstr_to_str(path) {
        Some(s) => s,
        None => return error_json("InvalidArgument", "invalid path"),
    };
    match stat::stat_path(path) {
        Ok(r) => match serde_json::to_string(&r) {
            Ok(s) => to_cstring(s),
            Err(e) => error_json("SerdeError", &e.to_string()),
        },
        Err(e) => error_json(e.code(), &e.to_string()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffr_c_classify(
    path: *const c_char,
    sniff_bytes: usize,
    full_open_max: u64,
    minified_threshold: usize,
) -> *mut c_char {
    let path = match cstr_to_str(path) {
        Some(s) => s,
        None => return error_json("InvalidArgument", "invalid path"),
    };
    match classify::classify_path(path, sniff_bytes, full_open_max, minified_threshold) {
        Ok(r) => match serde_json::to_string(&r) {
            Ok(s) => to_cstring(s),
            Err(e) => error_json("SerdeError", &e.to_string()),
        },
        Err(e) => error_json(e.code(), &e.to_string()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffr_c_read_chunk(
    path: *const c_char,
    chunk_id: u64,
    chunk_bytes: usize,
) -> *mut c_char {
    let path = match cstr_to_str(path) {
        Some(s) => s,
        None => return error_json("InvalidArgument", "invalid path"),
    };
    match read::read_chunk(path, chunk_id, chunk_bytes) {
        Ok(r) => match serde_json::to_string(&r) {
            Ok(s) => to_cstring(s),
            Err(e) => error_json("SerdeError", &e.to_string()),
        },
        Err(e) => error_json(e.code(), &e.to_string()),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ffr_c_read_lines(
    path: *const c_char,
    start_line: usize,
    end_line: usize,
) -> *mut c_char {
    let path = match cstr_to_str(path) {
        Some(s) => s,
        None => return error_json("InvalidArgument", "invalid path"),
    };
    match lines::read_lines(path, start_line, end_line) {
        Ok(r) => match serde_json::to_string(&r) {
            Ok(s) => to_cstring(s),
            Err(e) => error_json("SerdeError", &e.to_string()),
        },
        Err(e) => error_json(e.code(), &e.to_string()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ffr_c_version() -> *mut c_char {
    to_cstring(env!("CARGO_PKG_VERSION").to_string())
}
