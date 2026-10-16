//! Opening files a caller named, without being fooled by what they turn out to be.
//!
//! The helper runs as root and is handed paths by a process of the person. A path can be a link to a
//! file only root can read, a pipe that never gets a writer (an `open` that waits forever, with the lock
//! held), or a device. Every such file is opened without following a final link and without waiting,
//! and then looked at through the open handle: only a regular file is read.

use rustix::fs::OFlags;
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// The `open` flags that refuse a link in the last place and never block on a pipe.
pub fn untrusted_flags() -> i32 {
    (OFlags::NOFOLLOW | OFlags::NONBLOCK).bits() as i32
}

/// The `open` flag that refuses a link in the last place.
pub fn no_follow() -> i32 {
    OFlags::NOFOLLOW.bits() as i32
}

/// Opens `path` for reading if, and only if, it is a regular file.
pub fn open_regular(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(untrusted_flags())
        .open(path)?;
    if file.metadata()?.is_file() {
        Ok(file)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "it is not a regular file",
        ))
    }
}

/// What reading a file with a size limit found.
#[derive(Debug)]
pub enum Capped {
    /// The whole file.
    Bytes(Vec<u8>),
    /// More than the limit; nothing more was read.
    TooLarge,
}

/// Reads the regular file `path`, at most `limit` bytes.
pub fn read_capped(path: &Path, limit: usize) -> io::Result<Capped> {
    let mut bytes = Vec::new();
    open_regular(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok(if bytes.len() > limit {
        Capped::TooLarge
    } else {
        Capped::Bytes(bytes)
    })
}
