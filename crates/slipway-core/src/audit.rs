//! Checks every entry of a bundle against the signed release as it is unpacked: only listed
//! regular files with the listed digests, only plain directories, nothing else, nothing missing.

use crate::{
    digest::Sha256Hex,
    path::{BundlePath, PathError},
    release::{FileEntry, Release},
};
use std::collections::{BTreeMap, BTreeSet};

/// The kinds of archive entry that are refused outright.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    /// A symbolic link.
    Symlink,
    /// A hard link.
    HardLink,
    /// A device node.
    Device,
    /// A named pipe.
    Pipe,
    /// Anything else that is not a file or a directory.
    Other,
}

/// Why a bundle entry was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AuditError {
    /// The entry's path is not a plain relative path.
    #[error(transparent)]
    BadPath(#[from] PathError),
    /// The bundle holds a file the release does not list.
    #[error("the bundle holds a file the release does not list: {0}")]
    Unlisted(String),
    /// The bundle holds a folder none of the release's files are in.
    #[error("the bundle holds a folder the release does not need: {0}")]
    UnlistedDir(String),
    /// The bundle holds the same file twice.
    #[error("the bundle holds {0} twice")]
    Duplicate(String),
    /// A file's contents are not the ones the release lists.
    #[error("{0} does not match the release")]
    DigestMismatch(String),
    /// A file the release lists is not in the bundle.
    #[error("the bundle is missing {0}")]
    Missing(String),
    /// A link, device or other entry that must never be unpacked.
    #[error("the bundle holds an entry that is neither a file nor a folder: {path} ({kind:?})")]
    Forbidden {
        /// The entry's path.
        path: String,
        /// What it is.
        kind: EntryKind,
    },
    /// A file entry where the release has a folder, or the other way round.
    #[error("{0} is a file in one place and a folder in another")]
    Mixed(String),
}

/// The running check of one unpack.
#[derive(Debug)]
pub struct Audit<'a> {
    listed: BTreeMap<&'a str, &'a FileEntry>,
    seen: BTreeSet<&'a str>,
}

/// Strips the `./` prefixes and trailing `/` tar writers add. `None` is the archive root.
fn normalise(raw: &str) -> Option<&str> {
    let mut path = raw;
    while let Some(rest) = path.strip_prefix("./") {
        path = rest;
    }
    let path = path.trim_end_matches('/');
    (!path.is_empty() && path != ".").then_some(path)
}

impl<'a> Audit<'a> {
    /// Starts checking a bundle against `release`.
    pub fn new(release: &'a Release) -> Self {
        Self {
            listed: release.files.iter().map(|f| (f.path.as_str(), f)).collect(),
            seen: BTreeSet::new(),
        }
    }

    /// A directory entry. Allowed when it is the root or holds listed files.
    pub fn admit_dir(&mut self, raw: &str) -> Result<(), AuditError> {
        let Some(path) = normalise(raw) else {
            return Ok(());
        };
        BundlePath::parse(path)?;
        if self.listed.contains_key(path) {
            return Err(AuditError::Mixed(path.to_owned()));
        }
        let prefix = format!("{path}/");
        if self.listed.keys().any(|k| k.starts_with(&prefix)) {
            Ok(())
        } else {
            Err(AuditError::UnlistedDir(path.to_owned()))
        }
    }

    /// The release's entry for a file about to be unpacked: it must be listed and not yet seen.
    /// Call it before writing anything for the file.
    pub fn listing(&self, raw: &str) -> Result<&'a FileEntry, AuditError> {
        if raw.ends_with('/') {
            return Err(AuditError::Mixed(raw.to_owned()));
        }
        let path = normalise(raw).ok_or(AuditError::BadPath(PathError::Empty))?;
        BundlePath::parse(path)?;
        let entry = self
            .listed
            .get(path)
            .copied()
            .ok_or_else(|| AuditError::Unlisted(path.to_owned()))?;
        if self.seen.contains(path) {
            return Err(AuditError::Duplicate(path.to_owned()));
        }
        Ok(entry)
    }

    /// A regular file entry whose contents hash to `digest`. Returns the release's entry for it
    /// (the mode to give it).
    pub fn admit_file(
        &mut self,
        raw: &str,
        digest: &Sha256Hex,
    ) -> Result<&'a FileEntry, AuditError> {
        let entry = self.listing(raw)?;
        if &entry.sha256 != digest {
            return Err(AuditError::DigestMismatch(entry.path.to_string()));
        }
        self.seen.insert(entry.path.as_str());
        Ok(entry)
    }

    /// An entry of a kind that is never unpacked: always refused.
    pub fn refuse(&self, raw: &str, kind: EntryKind) -> AuditError {
        AuditError::Forbidden {
            path: raw.to_owned(),
            kind,
        }
    }

    /// Ends the check: every listed file must have been seen.
    pub fn finish(self) -> Result<(), AuditError> {
        match self.listed.keys().find(|k| !self.seen.contains(*k)) {
            Some(missing) => Err(AuditError::Missing((*missing).to_owned())),
            None => Ok(()),
        }
    }
}
