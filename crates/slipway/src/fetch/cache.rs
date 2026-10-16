//! Keeping the download cache small: one folder per version, and only the one being fetched is kept.

use crate::fsutil::remove_tree;
use semver::Version;
use std::io;
use std::path::Path;

/// Removes every version folder in `cache` except `keep`: earlier downloads that were superseded, or
/// that never finished, would otherwise pile up (a bundle is hundreds of megabytes). Anything in the
/// cache that is not a folder named like a version is left alone.
pub fn prune_cache(cache: &Path, keep: &Version) -> io::Result<()> {
    let entries = match std::fs::read_dir(cache) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let is_version = entry.file_type()?.is_dir()
            && entry
                .file_name()
                .to_str()
                .and_then(|name| Version::parse(name).ok())
                .is_some_and(|version| &version != keep);
        if is_version {
            remove_tree(&entry.path())?;
        }
    }
    Ok(())
}
