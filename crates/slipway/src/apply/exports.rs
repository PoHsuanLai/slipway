//! Makes the system's links match a release's exports.

use super::error::ApplyError;
use crate::fsutil::switch_link;
use crate::layout::Layout;
use semver::Version;
use slipway_core::{ExportAction, Found, Release, plan_exports};
use std::collections::BTreeMap;
use std::fs;
use std::io;

/// Reads the verified description kept in an installed release.
fn recorded(layout: &Layout, version: &Version) -> Result<Release, ApplyError> {
    let bytes = fs::read(layout.record(version)).map_err(ApplyError::io(format!(
        "reading the record of version {version}"
    )))?;
    Release::parse(&bytes).map_err(|e| ApplyError::Unpack(e.to_string()))
}

fn observe(layout: &Layout, link: &str) -> io::Result<Found> {
    let path = layout.export_path(link);
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() => Ok(Found::Link(fs::read_link(&path)?)),
        Ok(_) => Ok(Found::Foreign),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Found::Missing),
        Err(e) => Err(e),
    }
}

/// Brings the links in line with the exports of `to`, dropping those only `from` had. Returns the
/// links that were left alone because something else is in the way.
pub fn sync_exports(
    layout: &Layout,
    from: Option<&Version>,
    to: &Version,
) -> Result<Vec<String>, ApplyError> {
    let new = recorded(layout, to)?.exports;
    let old = match from {
        Some(version) => recorded(layout, version)
            .map(|r| r.exports)
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let mut found = BTreeMap::new();
    for export in new.iter().chain(old.iter()) {
        let seen = observe(layout, &export.link)
            .map_err(ApplyError::io(format!("looking at {}", export.link)))?;
        found.insert(export.link.clone(), seen);
    }
    let mut conflicts = Vec::new();
    for action in plan_exports(&layout.root, &new, &old, &found) {
        match action {
            ExportAction::Create { link, to } | ExportAction::Replace { link, to } => {
                let path = layout.export_path(&link);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(ApplyError::io(format!("making the folder for {link}")))?;
                }
                switch_link(&path, &to).map_err(ApplyError::io(format!("linking {link}")))?;
            }
            ExportAction::Remove { link } => {
                fs::remove_file(layout.export_path(&link))
                    .map_err(ApplyError::io(format!("removing {link}")))?;
            }
            ExportAction::Keep { .. } => {}
            ExportAction::Conflict { link } => conflicts.push(link),
        }
    }
    Ok(conflicts)
}
