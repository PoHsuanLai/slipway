//! One regular file the bundle installs.

use super::error::ReleaseError;
use crate::{digest::Sha256Hex, path::BundlePath};
use serde::Deserialize;

/// A file of the bundle: where it goes, its digest and its permission bits.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct FileEntry {
    /// Path relative to the release root.
    pub path: BundlePath,
    /// SHA-256 of the contents.
    pub sha256: Sha256Hex,
    /// Unix permission bits; owner-readable, never writable by group or others, no setuid.
    pub mode: u32,
}

impl FileEntry {
    /// Refuses modes that are not plain permission bits or that others could write.
    pub(super) fn check_mode(&self) -> Result<(), ReleaseError> {
        let plain = self.mode & !0o777 == 0;
        let others_write = self.mode & 0o022 != 0;
        let owner_reads = self.mode & 0o400 != 0;
        if plain && !others_write && owner_reads {
            Ok(())
        } else {
            Err(ReleaseError::BadFile {
                path: self.path.to_string(),
                why: format!("the mode {:o} is not allowed", self.mode),
            })
        }
    }
}
