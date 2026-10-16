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
use std::time::{Duration, Instant};

/// How long a held lock is waited for before the answer is "busy". A lock is per open file, and a
/// child forked by another thread holds a copy of it until it runs its program, so a lock just
/// let go can still read as held for a moment.
const LOCK_WAIT: Duration = Duration::from_secs(1);
/// How often a held lock is tried again within [`LOCK_WAIT`].
const LOCK_RETRY: Duration = Duration::from_millis(25);

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
        let until = Instant::now() + LOCK_WAIT;
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self { _file: file }),
                Err(TryLockError::WouldBlock) if Instant::now() < until => {
                    std::thread::sleep(LOCK_RETRY);
                }
                Err(TryLockError::WouldBlock) => return Err(ApplyError::Busy),
                Err(TryLockError::Error(source)) => {
                    return Err(ApplyError::Io {
                        what: "taking the update lock".to_owned(),
                        source,
                    });
                }
            }
        }
    }
}
