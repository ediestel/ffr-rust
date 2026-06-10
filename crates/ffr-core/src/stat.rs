use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::errors::FFRError;
use crate::types::StatPathResult;

pub fn stat_path(path: &str) -> Result<StatPathResult, FFRError> {
    let path_ref = Path::new(path);

    if !path_exists(path_ref) {
        return Ok(StatPathResult {
            exists: false,
            is_file: false,
            size: 0,
            mtime: 0,
            readonly: false,
        });
    }

    let is_file = is_regular_file(path_ref)?;
    let size = if is_file { file_size(path_ref)? } else { 0 };
    let mtime = file_mtime_unix(path_ref)?;
    let readonly = is_readonly(path_ref)?;

    Ok(StatPathResult {
        exists: true,
        is_file,
        size,
        mtime,
        readonly,
    })
}

pub fn path_exists(path: &Path) -> bool {
    path.exists()
}

pub fn is_regular_file(path: &Path) -> Result<bool, FFRError> {
    let metadata = fs::metadata(path)?;
    Ok(metadata.is_file())
}

pub fn file_size(path: &Path) -> Result<u64, FFRError> {
    let metadata = fs::metadata(path)?;
    Ok(metadata.len())
}

pub fn file_mtime_unix(path: &Path) -> Result<u64, FFRError> {
    let metadata = fs::metadata(path)?;
    let modified = metadata.modified()?;

    match modified.duration_since(UNIX_EPOCH) {
        Ok(duration) => Ok(duration.as_secs()),
        Err(_) => Err(FFRError::IOError(
            "file modified time is before UNIX_EPOCH".to_string(),
        )),
    }
}

pub fn is_readonly(path: &Path) -> Result<bool, FFRError> {
    let metadata = fs::metadata(path)?;
    Ok(metadata.permissions().readonly())
}
