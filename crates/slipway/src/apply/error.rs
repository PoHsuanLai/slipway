//! Why an apply failed, and the exit code `slipway-apply` gives for it.

use super::staged::StagedError;
use slipway_core::{AuditError, PlanError};
use std::io;

/// The helper's exit codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    /// Done.
    Done = 0,
    /// Failed for a reason on this computer (disk, permissions).
    Failed = 1,
    /// Wrong command line.
    Usage = 2,
    /// Refused: the release is not newer, or there is nothing to go back to.
    Refused = 3,
    /// The release did not check out: signature, digest or contents.
    Unverified = 4,
    /// Another update is running.
    Busy = 5,
    /// This computer has no key to check releases with.
    NoKey = 6,
}

/// Why an install or rollback did not happen.
#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    /// The staged release did not verify.
    #[error(transparent)]
    Staged(#[from] StagedError),
    /// The bundle's contents do not match the release.
    #[error("the bundle does not match the release: {0}")]
    Bundle(#[from] AuditError),
    /// The bundle could not be unpacked.
    #[error("the bundle could not be unpacked: {0}")]
    Unpack(String),
    /// No plan: not newer, nothing to roll back to.
    #[error(transparent)]
    Plan(#[from] PlanError),
    /// There is no key to check releases with.
    #[error("this computer has no key to check updates with")]
    NoKey,
    /// Another update holds the lock.
    #[error("another update is already running")]
    Busy,
    /// A file operation failed.
    #[error("{what}: {source}")]
    Io {
        /// What was being done.
        what: String,
        /// The error.
        source: io::Error,
    },
}

impl ApplyError {
    /// Wraps an I/O error with what was being done.
    pub fn io(what: impl Into<String>) -> impl FnOnce(io::Error) -> Self {
        let what = what.into();
        move |source| Self::Io { what, source }
    }

    /// The exit code for this failure.
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::NoKey => ExitCode::NoKey,
            Self::Staged(StagedError::Io { .. }) | Self::Io { .. } | Self::Unpack(_) => {
                ExitCode::Failed
            }
            Self::Staged(_) | Self::Bundle(_) => ExitCode::Unverified,
            Self::Plan(_) => ExitCode::Refused,
            Self::Busy => ExitCode::Busy,
        }
    }
}
