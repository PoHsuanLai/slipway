//! Unpacks a bundle into a fresh directory, checking every entry against the signed release.
//!
//! Only regular files and plain directories are ever created, each file is written to a fresh
//! path (never through a link), hashed as it is written, and given the mode the release lists.
//!
//! The new tree is private while it is built: its top folder is `0700` and every file starts as `0600`,
//! whatever the umask. Nothing but root can open anything in it until [`open_tree`] makes it public,
//! after the release record is in place. A process of the person cannot keep a handle on a file that is
//! later checked, or swap a folder for a link.

use super::error::ApplyError;
use crate::fsutil::{create_dirs, open_dirs, sync_tree};
use slipway_core::{Audit, EntryKind, Hasher, Release};
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use tar::EntryType;

/// The most bytes one unpack may write, whatever the archive claims.
pub const MAX_UNPACKED_BYTES: u64 = 16 << 30;

fn kind_of(kind: EntryType) -> EntryKind {
    if kind.is_symlink() {
        EntryKind::Symlink
    } else if kind.is_hard_link() {
        EntryKind::HardLink
    } else if kind.is_character_special() || kind.is_block_special() {
        EntryKind::Device
    } else if kind.is_fifo() {
        EntryKind::Pipe
    } else {
        EntryKind::Other
    }
}

/// Unpacks the (already verified) bundle read from `bundle` into the new directory `dest`, which must
/// not exist. On any refusal the caller removes `dest`. The tree is left private; see [`open_tree`].
pub fn unpack<R: Read>(bundle: R, release: &Release, dest: &Path) -> Result<(), ApplyError> {
    let decoder =
        zstd::stream::read::Decoder::new(bundle).map_err(ApplyError::io("reading the bundle"))?;
    let mut archive = tar::Archive::new(decoder);
    let mut audit = Audit::new(release);
    DirBuilder::new()
        .mode(0o700)
        .create(dest)
        .map_err(ApplyError::io("creating the new release folder"))?;
    let mut written = 0u64;
    let entries = archive
        .entries()
        .map_err(|e| ApplyError::Unpack(e.to_string()))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| ApplyError::Unpack(e.to_string()))?;
        let kind = entry.header().entry_type();
        if kind.is_pax_global_extensions() {
            continue;
        }
        let raw = entry
            .path()
            .map_err(|e| ApplyError::Unpack(e.to_string()))?
            .to_string_lossy()
            .into_owned();
        if kind.is_dir() {
            audit.admit_dir(&raw)?;
        } else if kind.is_file() {
            written = write_file(&mut audit, &mut entry, &raw, dest, written)?;
        } else {
            return Err(audit.refuse(&raw, kind_of(kind)).into());
        }
    }
    audit.finish()?;
    Ok(())
}

/// Makes a finished tree public: every folder `0755` (the top one last) and everything flushed to disk.
pub fn open_tree(dest: &Path) -> Result<(), ApplyError> {
    open_dirs(dest).map_err(ApplyError::io("opening the new release's folders"))?;
    sync_tree(dest).map_err(ApplyError::io("flushing the new release"))
}

fn write_file<R: Read>(
    audit: &mut Audit<'_>,
    entry: &mut R,
    raw: &str,
    dest: &Path,
    written: u64,
) -> Result<u64, ApplyError> {
    // The release must list the file before anything is written for it, but its digest is only
    // known once the contents have been read: write to the listed place, then admit.
    let listed = audit.listing(raw)?;
    let target = dest.join(listed.path.as_path());
    if let Some(parent) = target.parent() {
        create_dirs(parent, 0o755)
            .map_err(ApplyError::io("creating a folder in the new release"))?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&target)
        .map_err(ApplyError::io("creating a file in the new release"))?;
    let mut hasher = Hasher::new();
    let mut buffer = vec![0u8; 1 << 16];
    let mut written = written;
    loop {
        let n = entry
            .read(&mut buffer)
            .map_err(|e| ApplyError::Unpack(e.to_string()))?;
        if n == 0 {
            break;
        }
        written += n as u64;
        if written > MAX_UNPACKED_BYTES {
            return Err(ApplyError::Unpack(
                "the bundle unpacks to more than is allowed".to_owned(),
            ));
        }
        hasher.update(&buffer[..n]);
        file.write_all(&buffer[..n])
            .map_err(ApplyError::io("writing a file in the new release"))?;
    }
    let listing = audit.admit_file(raw, &hasher.finish())?;
    file.set_permissions(fs::Permissions::from_mode(listing.mode))
        .map_err(ApplyError::io("setting a file's permissions"))?;
    file.sync_all().map_err(ApplyError::io("flushing a file"))?;
    Ok(written)
}
