//! The folder only root can open, and the unnamed private copy of a bundle kept in it.
//!
//! A bundle is read once, from the file the caller named, into a file that has no name and that nobody
//! else can reach. The digest is taken while it is copied and the unpack reads the copy, so the caller
//! cannot change the bytes between the check and the use.

use super::error::ApplyError;
use rustix::process::geteuid;
use std::fs::{self, DirBuilder, File, OpenOptions, Permissions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

/// The name the private copy has for the moment between creating it and removing the name.
const VAULT_NAME: &str = "bundle.copy";

/// Makes sure `path` is a folder of ours that nobody else can enter. A folder made by another user, or
/// a link in its place, is refused.
pub fn ensure_private_dir(path: &Path) -> Result<(), ApplyError> {
    let what = "preparing the private folder";
    match DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(ApplyError::io(what)(e)),
    }
    let meta = fs::symlink_metadata(path).map_err(ApplyError::io(what))?;
    if !meta.is_dir() || meta.uid() != geteuid().as_raw() {
        return Err(ApplyError::io(what)(io::Error::other(
            "it is not a folder of this program's",
        )));
    }
    fs::set_permissions(path, Permissions::from_mode(0o700)).map_err(ApplyError::io(what))
}

/// A new, empty file in `dir` that is readable and writable by us alone and has no name: the name is
/// removed at once, so nothing can open it again.
pub fn open_vault(dir: &Path) -> io::Result<File> {
    let path = dir.join(VAULT_NAME);
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    fs::remove_file(&path)?;
    Ok(file)
}

/// The bytes free for a program that is not root on the file system holding `dir`.
pub fn free_bytes(dir: &Path) -> io::Result<u64> {
    let stat = rustix::fs::statvfs(dir)?;
    Ok(stat.f_bavail.saturating_mul(stat.f_frsize))
}
