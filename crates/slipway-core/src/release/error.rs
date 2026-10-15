//! Why a release index was refused.

use crate::path::PathError;

/// Why `release.toml` was not accepted.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReleaseError {
    /// The text is not UTF-8, is too large, or is not the expected TOML.
    #[error("the release description is not readable: {0}")]
    Unreadable(String),
    /// The format number is one this updater does not know.
    #[error("the release description has a format this updater does not know ({0})")]
    UnsupportedFormat(u32),
    /// The version is not a plain semantic version.
    #[error("the release version is not valid: {0}")]
    BadVersion(String),
    /// The date is not an RFC 3339 time.
    #[error("the release date is not valid: {0}")]
    BadDate(String),
    /// The notes are longer than a Software Update page shows.
    #[error("the release notes are too long")]
    NotesTooLong,
    /// The bundle entry is unusable (name, size).
    #[error("the release bundle entry is not valid: {0}")]
    BadBundle(String),
    /// A listed path is refused.
    #[error(transparent)]
    Path(#[from] PathError),
    /// A file entry is unusable.
    #[error("the release file entry for {path} is not valid: {why}")]
    BadFile {
        /// The entry's path.
        path: String,
        /// What is wrong.
        why: String,
    },
    /// The same path is listed twice.
    #[error("the release lists {0} twice")]
    DuplicateFile(String),
    /// One listed file is inside another listed file.
    #[error("the release lists {0} both as a file and as a folder")]
    FileInsideFile(String),
    /// The release lists no files.
    #[error("the release lists no files")]
    NoFiles,
    /// An export link is outside the allowed places or malformed.
    #[error("the release asks for a link in a place that is not allowed: {0}")]
    BadExportLink(String),
    /// The same link is listed twice.
    #[error("the release lists the link {0} twice")]
    DuplicateExport(String),
    /// An export names something that is not a file the bundle holds (a folder does not count).
    #[error("the release links to {0}, which is not a file the bundle holds")]
    ExportTargetMissing(String),
}
