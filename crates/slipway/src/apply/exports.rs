//! Makes the system's links match a release's exports.

use super::error::ApplyError;
use super::open::{Capped, read_capped};
use super::staged::StagedError;
use super::synced::Synced;
use crate::fsutil::switch_link;
use crate::layout::Layout;
use semver::Version;
use slipway_core::release::MAX_INDEX_BYTES;
use slipway_core::{Authenticator, ExportAction, Found, Release, SignError, plan_exports};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

/// The longest signature text read.
const MAX_SIGNATURE_BYTES: usize = 4096;

fn read_record_file(path: &Path, limit: usize, version: &Version) -> Result<Vec<u8>, ApplyError> {
    let what = format!("reading the record of version {version}");
    match read_capped(path, limit).map_err(ApplyError::io(what))? {
        Capped::Bytes(bytes) => Ok(bytes),
        Capped::TooLarge => Err(ApplyError::Unpack(format!(
            "the record of version {version} is too large"
        ))),
    }
}

/// Reads the description kept in an installed release and checks its signature again with `auth`:
/// what is on disk is trusted only as far as the key on this computer vouches for it.
pub(super) fn recorded(
    layout: &Layout,
    auth: &dyn Authenticator,
    version: &Version,
) -> Result<Release, ApplyError> {
    let bytes = read_record_file(&layout.record(version), MAX_INDEX_BYTES, version)?;
    let signature = String::from_utf8(read_record_file(
        &layout.record_signature(version),
        MAX_SIGNATURE_BYTES,
        version,
    )?)
    .map_err(|_| StagedError::Signature(SignError::BadSignature))?;
    auth.authenticate(&bytes, &signature)
        .map_err(StagedError::Signature)?;
    Ok(Release::parse(&bytes).map_err(StagedError::Release)?)
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

/// Brings the links in line with the exports of `to`, dropping those only `from` had. Reports the links
/// that were left alone because something else is in the way. A record of the release being left that
/// does not verify is not used: its links are left as they are, and that is said.
pub fn sync_exports(
    layout: &Layout,
    auth: &dyn Authenticator,
    from: Option<&Version>,
    to: &Version,
) -> Result<Synced, ApplyError> {
    let mut synced = Synced::default();
    let new = recorded(layout, auth, to)?.exports;
    let old = match from.map(|version| (version, recorded(layout, auth, version))) {
        Some((_, Ok(record))) => record.exports,
        Some((version, Err(why))) => {
            synced.warnings.push(format!(
                "the links of version {version} were left as they are: {why}"
            ));
            Vec::new()
        }
        None => Vec::new(),
    };
    let mut found = BTreeMap::new();
    for export in new.iter().chain(old.iter()) {
        let seen = observe(layout, &export.link)
            .map_err(ApplyError::io(format!("looking at {}", export.link)))?;
        found.insert(export.link.clone(), seen);
    }
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
            ExportAction::Conflict { link } => synced.conflicts.push(link),
        }
    }
    Ok(synced)
}
