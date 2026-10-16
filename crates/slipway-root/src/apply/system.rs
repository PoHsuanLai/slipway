//! Makes the system files on this computer match a release's `[[system]]` list.
//!
//! Each file is read from the release's own tree through a handle this code opens, hashed against the
//! digest the signed description lists, and only then written; what is placed is exactly what was
//! signed, whatever happened to the tree since it was unpacked. A file is replaced or removed only
//! when it still holds what an earlier release placed; anything else at a destination, including a file
//! of the distribution's that the new one would hide, is left alone and reported.

use super::error::ApplyError;
use super::exports::recorded;
use super::open::{Capped, read_capped};
use super::relabel::{Reach, Relabel, Relabeled};
use super::synced::Synced;
use crate::fsutil::write_atomic_mode;
use crate::layout::Layout;
use semver::Version;
use slipway_core::{
    Authenticator, Placement, Sha256Hex, SystemAction, SystemFound, placements, plan_system,
    vendor_copies,
};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

/// The largest system file read: ours are small, and a larger existing one is not ours.
const MAX_SYSTEM_FILE_BYTES: u64 = 1 << 20;

fn placed(
    layout: &Layout,
    auth: &dyn Authenticator,
    version: &Version,
) -> Result<Vec<Placement>, ApplyError> {
    placements(&recorded(layout, auth, version)?).map_err(|e| ApplyError::Unpack(e.to_string()))
}

fn exists(layout: &Layout, path: &str) -> io::Result<bool> {
    match fs::symlink_metadata(layout.export_path(path)) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// What is at `path`. A destination that is empty but would hide a file the distribution ships under
/// the same name counts as taken: on Fedora and Arch the vendor's PAM, udev and polkit files live in
/// `/usr/lib`, so `/etc/...` looks free when it is not, and placing ours would replace the vendor's
/// file without anyone having asked. A file is only ever replaced when an earlier release placed it.
fn observe(layout: &Layout, path: &str) -> io::Result<SystemFound> {
    let at = layout.export_path(path);
    match fs::symlink_metadata(&at) {
        Ok(meta) if meta.file_type().is_file() && meta.len() <= MAX_SYSTEM_FILE_BYTES => {
            Ok(SystemFound::File(Sha256Hex::of(&fs::read(&at)?)))
        }
        Ok(_) => Ok(SystemFound::Foreign),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            for vendor in vendor_copies(path) {
                if exists(layout, &vendor)? {
                    return Ok(SystemFound::Foreign);
                }
            }
            Ok(SystemFound::Missing)
        }
        Err(e) => Err(e),
    }
}

/// The contents to place for `item`, read from the release's tree and checked against the digest the
/// signed description lists.
fn contents(layout: &Layout, to: &Version, item: &Placement) -> Result<Vec<u8>, ApplyError> {
    let source = layout.release_dir(to).join(item.source.as_path());
    let what = format!("reading the contents of {}", item.path);
    let changed = || ApplyError::Changed(item.path.clone());
    match read_capped(&source, MAX_SYSTEM_FILE_BYTES as usize).map_err(ApplyError::io(what))? {
        Capped::Bytes(bytes) if Sha256Hex::of(&bytes) == item.sha256 => Ok(bytes),
        Capped::Bytes(_) | Capped::TooLarge => Err(changed()),
    }
}

/// Brings the system files in line with those of `to`, dropping those only `from` had.
pub fn sync_system(
    layout: &Layout,
    auth: &dyn Authenticator,
    from: Option<&Version>,
    to: &Version,
    relabel: &dyn Relabel,
) -> Result<Synced, ApplyError> {
    let mut synced = Synced::default();
    let new = placed(layout, auth, to)?;
    let old = match from.map(|version| (version, placed(layout, auth, version))) {
        Some((_, Ok(list))) => list,
        Some((version, Err(why))) => {
            synced.warnings.push(format!(
                "the system files of version {version} were left as they are: {why}"
            ));
            Vec::new()
        }
        None => Vec::new(),
    };
    let mut found = BTreeMap::new();
    for item in new.iter().chain(old.iter()) {
        let seen = observe(layout, &item.path)
            .map_err(ApplyError::io(format!("looking at {}", item.path)))?;
        found.insert(item.path.clone(), seen);
    }
    let mut written: Vec<PathBuf> = Vec::new();
    for action in plan_system(&new, &old, &found) {
        match action {
            SystemAction::Place(item) => {
                let bytes = contents(layout, to, &item)?;
                let at = layout.export_path(&item.path);
                write_atomic_mode(&at, &bytes, item.mode)
                    .map_err(ApplyError::io(format!("placing {}", item.path)))?;
                written.push(at);
            }
            SystemAction::Remove { path } => {
                fs::remove_file(layout.export_path(&path))
                    .map_err(ApplyError::io(format!("removing {path}")))?;
            }
            SystemAction::Keep { .. } => {}
            SystemAction::Conflict { path } => synced.conflicts.push(path),
        }
    }
    if !written.is_empty()
        && let Relabeled::Failed(why) = relabel.relabel(&written, Reach::Paths)
    {
        synced.warnings.push(format!(
            "the security labels of the system files could not be set: {why}"
        ));
    }
    Ok(synced)
}
