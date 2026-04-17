//! Thread-safe shared handles wrapping Option<T> with RwLock.
//!
//! Mirrors the `SharedFrecency` / `SharedPicker` pattern from fff-core.
//! Used for singletons that are initialized lazily and may be replaced
//! (e.g. when the caller reconfigures the metadata DB path).

use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::db::MetadataDb;
use crate::errors::FFRError;
use crate::lines::LineIndex;
use std::collections::HashMap;

/// Shared handle to the metadata LMDB store.
#[derive(Clone, Default)]
pub struct SharedMetadata(pub(crate) Arc<RwLock<Option<MetadataDb>>>);

impl std::fmt::Debug for SharedMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SharedMetadata").field(&"..").finish()
    }
}

impl SharedMetadata {
    pub fn read(&self) -> Result<RwLockReadGuard<'_, Option<MetadataDb>>, FFRError> {
        self.0
            .read()
            .map_err(|_| FFRError::Internal("metadata lock poisoned".to_string()))
    }

    pub fn write(&self) -> Result<RwLockWriteGuard<'_, Option<MetadataDb>>, FFRError> {
        self.0
            .write()
            .map_err(|_| FFRError::Internal("metadata lock poisoned".to_string()))
    }

    pub fn init(&self, db: MetadataDb) -> Result<(), FFRError> {
        let mut guard = self.write()?;
        *guard = Some(db);
        Ok(())
    }

    pub fn take(&self) -> Result<Option<MetadataDb>, FFRError> {
        let mut guard = self.write()?;
        Ok(guard.take())
    }

    pub fn is_initialized(&self) -> bool {
        self.read().map(|g| g.is_some()).unwrap_or(false)
    }
}

/// Shared handle to the ephemeral LineIndex cache.
#[derive(Clone, Default)]
pub struct SharedLineIndexCache(pub(crate) Arc<RwLock<HashMap<String, LineIndex>>>);

impl std::fmt::Debug for SharedLineIndexCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SharedLineIndexCache").field(&"..").finish()
    }
}

impl SharedLineIndexCache {
    pub fn read(&self) -> Result<RwLockReadGuard<'_, HashMap<String, LineIndex>>, FFRError> {
        self.0
            .read()
            .map_err(|_| FFRError::Internal("line index cache lock poisoned".to_string()))
    }

    pub fn write(&self) -> Result<RwLockWriteGuard<'_, HashMap<String, LineIndex>>, FFRError> {
        self.0
            .write()
            .map_err(|_| FFRError::Internal("line index cache lock poisoned".to_string()))
    }
}
