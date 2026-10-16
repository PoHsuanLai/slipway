//! What went wrong, in words a person can act on. Nothing here names a protocol or a tool.

use crate::elevate::ApplyFailure;
use crate::fetch::{FeedError, FetchError, StageError};
use crate::keys::KeyError;

/// A failure, before it is put into words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    /// There is no key to check releases with.
    NoKey,
    /// The key is there but unreadable.
    KeyUnreadable,
    /// The release site could not be reached.
    Offline,
    /// What arrived could not be verified.
    Unverified,
    /// The person (or the system) did not allow the install.
    NotAllowed,
    /// Something is already being done.
    Busy,
    /// There is nothing to install.
    NothingToInstall,
    /// There is nothing to go back to.
    NothingToRestore,
    /// The version a caller named is not a version at all.
    NotAVersion,
    /// The update on offer is not the one the caller showed the person.
    VersionChanged,
    /// A program that may not start an update asked to install one.
    InstallNotFromHere,
    /// A program that may not ask to go back asked to.
    RollbackNotFromHere,
    /// The program that asked could not be told apart from any other.
    CallerUnknown,
    /// Anything else.
    Other,
}

impl Problem {
    /// The sentence shown to the person.
    pub fn plain(self) -> &'static str {
        match self {
            Self::NoKey => {
                "This computer cannot tell whether an update is genuine, so it will not install updates."
            }
            Self::KeyUnreadable => {
                "The key that proves an update is genuine could not be read, so updates will not be installed."
            }
            Self::Offline => {
                "Updates could not be checked. Make sure you are online and try again."
            }
            Self::Unverified => "The update could not be verified, so it was not installed.",
            Self::NotAllowed => "The update was not allowed to install.",
            Self::Busy => "An update is already in progress.",
            Self::NothingToInstall => "There is no update to install.",
            Self::NothingToRestore => "There is no earlier version to go back to.",
            Self::NotAVersion => "That is not a version of the software.",
            Self::VersionChanged => {
                "A different update is on offer now. Look at it again before installing."
            }
            Self::InstallNotFromHere => "Updates are installed from Software Update in Settings.",
            Self::RollbackNotFromHere => {
                "Going back to an earlier version is done from Settings, where you will be asked for your password."
            }
            Self::CallerUnknown => {
                "The program that asked could not be recognised, so nothing was done."
            }
            Self::Other => "The update could not be installed. Try again later.",
        }
    }

    /// The stable slug programs read (`ProblemKind` on the bus). Never shown to a person; it never
    /// changes once released, so a program can match on it where it must not match on [`Self::plain`].
    pub fn kind(self) -> &'static str {
        match self {
            Self::NoKey => "no-key",
            Self::KeyUnreadable => "key-unreadable",
            Self::Offline => "offline",
            Self::Unverified => "unverified",
            Self::NotAllowed => "not-allowed",
            Self::Busy => "busy",
            Self::NothingToInstall => "nothing-to-install",
            Self::NothingToRestore => "nothing-to-restore",
            Self::NotAVersion => "not-a-version",
            Self::VersionChanged => "version-changed",
            Self::InstallNotFromHere => "install-not-from-here",
            Self::RollbackNotFromHere => "rollback-not-from-here",
            Self::CallerUnknown => "caller-unknown",
            Self::Other => "other",
        }
    }

    /// From the exit status of the privileged helper (see `slipway-apply --help`).
    pub fn from_apply(failure: &ApplyFailure) -> Self {
        match failure.code {
            Some(126 | 127) => Self::NotAllowed,
            Some(3) => Self::NothingToInstall,
            Some(4) => Self::Unverified,
            Some(5) => Self::Busy,
            Some(6) => Self::NoKey,
            _ => Self::Other,
        }
    }
}

impl From<&KeyError> for Problem {
    fn from(e: &KeyError) -> Self {
        match e {
            KeyError::Missing => Self::NoKey,
            KeyError::Unreadable => Self::KeyUnreadable,
        }
    }
}

impl From<&FeedError> for Problem {
    fn from(e: &FeedError) -> Self {
        match e {
            FeedError::Fetch(_) => Self::Offline,
            FeedError::Signature(_) | FeedError::Release(_) => Self::Unverified,
        }
    }
}

impl From<&StageError> for Problem {
    fn from(e: &StageError) -> Self {
        match e {
            StageError::Fetch(FetchError::Save(_)) => Self::Other,
            StageError::Fetch(_) => Self::Offline,
            StageError::Mismatch => Self::Unverified,
        }
    }
}
