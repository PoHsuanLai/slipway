//! `release.toml`: the signed description of one release, parsed and checked.

mod error;
mod export;
mod file;
mod system;

pub use error::ReleaseError;
pub use export::{ALLOWED_LINK_DIRS, Export};
pub use file::FileEntry;
pub use system::{ALLOWED_SYSTEM_DIRS, SYSTEM_TREE, SystemFile};

use crate::{digest::Sha256Hex, schedule::UnixSeconds};
use semver::Version;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/// The one format number this version understands.
pub const FORMAT: u32 = 1;
/// The largest `release.toml` read. Each listed file costs about 200 bytes, so this holds the
/// largest allowed release ([`MAX_FILES`]) with room for the notes and components.
pub const MAX_INDEX_BYTES: usize = 24 << 20;
/// The longest notes text.
pub const MAX_NOTES_BYTES: usize = 16 * 1024;
/// The largest bundle accepted, in bytes.
pub const MAX_BUNDLE_BYTES: u64 = 8 << 30;
/// The most files one release may list.
pub const MAX_FILES: usize = 100_000;

/// Where the bundle is and how to check it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct BundleRef {
    /// The asset's file name next to `release.toml`.
    pub name: String,
    /// SHA-256 of the whole bundle file.
    pub sha256: Sha256Hex,
    /// Its size in bytes.
    pub size: u64,
}

/// A parsed, validated `release.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// The release's version.
    pub version: Version,
    /// When it was published.
    pub released: UnixSeconds,
    /// Plain text for the Software Update page.
    pub notes: String,
    /// The bundle to fetch.
    pub bundle: BundleRef,
    /// Which revision of each repository went in (informational).
    pub components: BTreeMap<String, String>,
    /// Every regular file in the bundle.
    pub files: Vec<FileEntry>,
    /// Links the system needs into the release.
    pub exports: Vec<Export>,
    /// Files the privileged helper copies into system folders.
    pub system: Vec<SystemFile>,
}

#[derive(Deserialize)]
struct Raw {
    format: u32,
    version: String,
    date: String,
    #[serde(default)]
    notes: String,
    bundle: BundleRef,
    #[serde(default)]
    components: BTreeMap<String, String>,
    #[serde(default)]
    file: Vec<FileEntry>,
    #[serde(default)]
    export: Vec<Export>,
    #[serde(default)]
    system: Vec<SystemFile>,
}

impl Release {
    /// Parses and validates the bytes of a `release.toml`. Call it only on bytes whose signature
    /// has been checked.
    pub fn parse(bytes: &[u8]) -> Result<Self, ReleaseError> {
        if bytes.len() > MAX_INDEX_BYTES {
            return Err(ReleaseError::Unreadable("it is too large".to_owned()));
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|_| ReleaseError::Unreadable("it is not text".to_owned()))?;
        let raw: Raw = toml::from_str(text).map_err(|e| ReleaseError::Unreadable(e.to_string()))?;
        Self::from_raw(raw)
    }

    fn from_raw(raw: Raw) -> Result<Self, ReleaseError> {
        if raw.format != FORMAT {
            return Err(ReleaseError::UnsupportedFormat(raw.format));
        }
        let version = Version::parse(&raw.version)
            .ok()
            .filter(|v| v.build.is_empty())
            .ok_or_else(|| ReleaseError::BadVersion(raw.version.clone()))?;
        let released = OffsetDateTime::parse(&raw.date, &Rfc3339)
            .map(|d| UnixSeconds(d.unix_timestamp()))
            .map_err(|_| ReleaseError::BadDate(raw.date.clone()))?;
        if raw.notes.len() > MAX_NOTES_BYTES {
            return Err(ReleaseError::NotesTooLong);
        }
        check_bundle(&raw.bundle)?;
        check_files(&raw.file)?;
        check_exports(&raw.export, &raw.file)?;
        check_system(&raw.system, &raw.file)?;
        Ok(Self {
            version,
            released,
            notes: raw.notes,
            bundle: raw.bundle,
            components: raw.components,
            files: raw.file,
            exports: raw.export,
            system: raw.system,
        })
    }

    /// The entry for `path`, if the release lists it.
    pub fn file(&self, path: &str) -> Option<&FileEntry> {
        self.files.iter().find(|f| f.path.as_str() == path)
    }
}

fn check_bundle(bundle: &BundleRef) -> Result<(), ReleaseError> {
    let plain_name = !bundle.name.is_empty()
        && !bundle.name.starts_with('.')
        && bundle
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+'));
    if !plain_name {
        return Err(ReleaseError::BadBundle(format!(
            "the name {:?}",
            bundle.name
        )));
    }
    if bundle.size == 0 || bundle.size > MAX_BUNDLE_BYTES {
        return Err(ReleaseError::BadBundle(format!("the size {}", bundle.size)));
    }
    Ok(())
}

fn check_files(files: &[FileEntry]) -> Result<(), ReleaseError> {
    if files.is_empty() {
        return Err(ReleaseError::NoFiles);
    }
    if files.len() > MAX_FILES {
        return Err(ReleaseError::BadBundle("too many files".to_owned()));
    }
    let mut seen = BTreeSet::new();
    for file in files {
        file.check_mode()?;
        if !seen.insert(file.path.as_str()) {
            return Err(ReleaseError::DuplicateFile(file.path.to_string()));
        }
    }
    for file in files {
        if let Some(inside) = file.path.ancestors().into_iter().find(|a| seen.contains(a)) {
            return Err(ReleaseError::FileInsideFile(inside.to_owned()));
        }
    }
    Ok(())
}

fn check_exports(exports: &[Export], files: &[FileEntry]) -> Result<(), ReleaseError> {
    let mut seen = BTreeSet::new();
    for export in exports {
        export.check_link()?;
        if !seen.insert(export.link.as_str()) {
            return Err(ReleaseError::DuplicateExport(export.link.clone()));
        }
        if !files.iter().any(|f| f.path == export.target) {
            return Err(ReleaseError::ExportTargetMissing(export.target.to_string()));
        }
    }
    Ok(())
}

fn check_system(system: &[SystemFile], files: &[FileEntry]) -> Result<(), ReleaseError> {
    let mut seen = BTreeSet::new();
    for file in system {
        file.check_path()?;
        if !seen.insert(file.path.as_str()) {
            return Err(ReleaseError::DuplicateSystemFile(file.path.clone()));
        }
        let source = file.source()?;
        if !files.iter().any(|f| f.path == source) {
            return Err(ReleaseError::SystemSourceMissing(file.path.clone()));
        }
    }
    // Everything in the bundle's system folder must be listed: it is a place only listed files go.
    let tree = format!("{SYSTEM_TREE}/");
    for file in files.iter().filter(|f| f.path.as_str().starts_with(&tree)) {
        let dest = &file.path.as_str()[SYSTEM_TREE.len()..];
        if !seen.contains(dest) {
            return Err(ReleaseError::SystemFileUnlisted(dest.to_owned()));
        }
    }
    Ok(())
}
