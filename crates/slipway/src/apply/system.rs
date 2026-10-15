//! Makes the system files on this computer match a release's `[[system]]` list.
//!
//! Each file is copied out of the release's own (already verified) tree, so what is placed is exactly
//! what was signed. A file is replaced or removed only when it still holds what an earlier release
//! placed; anything else at a destination is left alone and reported.

use super::error::ApplyError;
use super::exports::recorded;
use super::relabel::{Reach, Relabel, Relabeled};
use crate::fsutil::write_atomic_mode;
use crate::layout::Layout;
use semver::Version;
use slipway_core::{Placement, Sha256Hex, SystemAction, SystemFound, placements, plan_system};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

/// The largest existing file that is read to compare it: a system file of ours is small.
const MAX_COMPARED_BYTES: u64 = 1 << 20;

/// What a system step did.
#[derive(Debug, Default)]
pub struct Synced {
    /// Destinations left alone because something else is there.
    pub conflicts: Vec<String>,
    /// Plain notes about labels that could not be set.
    pub warnings: Vec<String>,
}

fn placed(layout: &Layout, version: &Version) -> Result<Vec<Placement>, ApplyError> {
    placements(&recorded(layout, version)?).map_err(|e| ApplyError::Unpack(e.to_string()))
}

fn observe(layout: &Layout, path: &str) -> io::Result<SystemFound> {
    let at = layout.export_path(path);
    match fs::symlink_metadata(&at) {
        Ok(meta) if meta.file_type().is_file() && meta.len() <= MAX_COMPARED_BYTES => {
            Ok(SystemFound::File(Sha256Hex::of(&fs::read(&at)?)))
        }
        Ok(_) => Ok(SystemFound::Foreign),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(SystemFound::Missing),
        Err(e) => Err(e),
    }
}

/// Brings the system files in line with those of `to`, dropping those only `from` had.
pub fn sync_system(
    layout: &Layout,
    from: Option<&Version>,
    to: &Version,
    relabel: &dyn Relabel,
) -> Result<Synced, ApplyError> {
    let new = placed(layout, to)?;
    let old = match from {
        Some(version) => placed(layout, version).unwrap_or_default(),
        None => Vec::new(),
    };
    let mut found = BTreeMap::new();
    for item in new.iter().chain(old.iter()) {
        let seen = observe(layout, &item.path)
            .map_err(ApplyError::io(format!("looking at {}", item.path)))?;
        found.insert(item.path.clone(), seen);
    }
    let mut synced = Synced::default();
    let mut written: Vec<PathBuf> = Vec::new();
    for action in plan_system(&new, &old, &found) {
        match action {
            SystemAction::Place(item) => {
                let bytes = fs::read(layout.release_dir(to).join(item.source.as_path())).map_err(
                    ApplyError::io(format!("reading the contents of {}", item.path)),
                )?;
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
