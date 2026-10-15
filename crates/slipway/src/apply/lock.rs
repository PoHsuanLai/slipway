//! One install at a time: an advisory lock on `<root>/lock`.

use super::error::ApplyError;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

/// Holds the lock until dropped.
#[derive(Debug)]
pub struct Lock {
    _file: File,
}

impl Lock {
    /// Takes the lock at `path`, or says another update has it.
    pub fn take(path: &Path) -> Result<Self, ApplyError> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .map_err(ApplyError::io("opening the update lock"))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(ApplyError::Busy),
            Err(TryLockError::Error(source)) => Err(ApplyError::Io {
                what: "taking the update lock".to_owned(),
                source,
            }),
        }
    }
}
