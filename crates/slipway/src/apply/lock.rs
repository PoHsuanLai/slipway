//! One install at a time: an advisory lock in the private folder.
//!
//! The lock file is in a folder only root can enter. `flock` works on a file opened only for reading, so
//! a lock file anybody can open could be held by anybody, for good, and every update would answer "busy".

use super::error::ApplyError;
use super::open::no_follow;
use super::private::ensure_private_dir;
use crate::layout::Layout;
use std::fs::{File, OpenOptions, TryLockError};
use std::os::unix::fs::OpenOptionsExt;

/// Holds the lock until dropped.
#[derive(Debug)]
pub struct Lock {
    _file: File,
}

impl Lock {
    /// Takes the lock under `layout`'s root, or says another update has it.
    pub fn take(layout: &Layout) -> Result<Self, ApplyError> {
        ensure_private_dir(&layout.private_dir())?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .custom_flags(no_follow())
            .open(layout.lock_file())
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
