//! Which system files to place, keep or remove so they match a release.
//!
//! Like [`crate::exports`], the caller looks at the file system and reports what it found at each
//! destination; this module only decides. A file is overwritten or removed only when its contents are
//! exactly what an earlier release of ours placed there. Anything else at a destination is the
//! person's or the system's, and is left alone.

use crate::{
    digest::Sha256Hex,
    path::BundlePath,
    release::{Release, ReleaseError},
};
use std::collections::BTreeMap;

/// One system file of a release, with what is needed to place it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    /// The absolute destination.
    pub path: String,
    /// Where its contents are inside the release.
    pub source: BundlePath,
    /// The digest of those contents.
    pub sha256: Sha256Hex,
    /// The permission bits the file gets.
    pub mode: u32,
}

/// What was found at a destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemFound {
    /// Nothing there.
    Missing,
    /// A regular file with this digest.
    File(Sha256Hex),
    /// A link, a folder, a file too large to be ours, or anything else.
    Foreign,
}

/// What to do about one destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemAction {
    /// Write the file (new, or replacing one an earlier release placed).
    Place(Placement),
    /// Already right.
    Keep {
        /// The destination.
        path: String,
    },
    /// Remove a file an earlier release placed and this one no longer lists.
    Remove {
        /// The destination.
        path: String,
    },
    /// Something else is in the way; leave it and tell the person.
    Conflict {
        /// The destination.
        path: String,
    },
}

/// The system files of `release`, each with its digest and mode taken from the listed bundle file.
pub fn placements(release: &Release) -> Result<Vec<Placement>, ReleaseError> {
    release
        .system
        .iter()
        .map(|file| {
            let source = file.source()?;
            let entry = release
                .file(source.as_str())
                .ok_or_else(|| ReleaseError::SystemSourceMissing(file.path.clone()))?;
            Ok(Placement {
                path: file.path.clone(),
                source,
                sha256: entry.sha256.clone(),
                mode: entry.mode,
            })
        })
        .collect()
}

/// Plans the system files for the release `new`, coming from the release whose files were `old`.
/// `found` says what is at each destination of either list (a missing key counts as nothing there).
pub fn plan_system(
    new: &[Placement],
    old: &[Placement],
    found: &BTreeMap<String, SystemFound>,
) -> Vec<SystemAction> {
    let at = |path: &str| found.get(path).cloned().unwrap_or(SystemFound::Missing);
    let ours_before =
        |path: &str, digest: &Sha256Hex| old.iter().any(|o| o.path == path && &o.sha256 == digest);
    let mut actions = Vec::new();
    for want in new {
        actions.push(match at(&want.path) {
            SystemFound::Missing => SystemAction::Place(want.clone()),
            SystemFound::File(have) if have == want.sha256 => SystemAction::Keep {
                path: want.path.clone(),
            },
            SystemFound::File(have) if ours_before(&want.path, &have) => {
                SystemAction::Place(want.clone())
            }
            SystemFound::File(_) | SystemFound::Foreign => SystemAction::Conflict {
                path: want.path.clone(),
            },
        });
    }
    for gone in old.iter().filter(|o| !new.iter().any(|n| n.path == o.path)) {
        if matches!(at(&gone.path), SystemFound::File(have) if have == gone.sha256) {
            actions.push(SystemAction::Remove {
                path: gone.path.clone(),
            });
        }
    }
    actions
}
