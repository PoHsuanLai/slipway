//! The root side of the health check: how many boots a release has been started in without the
//! session ever vouching for it.
//!
//! The person's own state file cannot decide a rollback that needs no password, because any process of
//! theirs can write it. This record is kept by the privileged helper, in a folder only root can open,
//! and counts only what root can see itself: the kernel's boot identifier. A release that has been
//! started in more than [`MAX_UNCONFIRMED_BOOTS`] different boots and never confirmed is rolled back;
//! a process of the person cannot make that happen without restarting the computer that many times.

use crate::health::Restore;
use semver::Version;
use serde::{Deserialize, Serialize};

/// How many different boots a release gets to be confirmed in. The next unconfirmed boot restores the
/// previous release instead of starting the session again.
pub const MAX_UNCONFIRMED_BOOTS: usize = 2;
/// The most boot identifiers remembered, whatever happens.
const KEEP_BOOTS: usize = 8;

/// Whether the session has vouched for the release.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trust {
    /// No session has said it started well.
    #[default]
    Unconfirmed,
    /// A session started well on it, or it was restored by hand or by this check.
    Confirmed,
}

/// What the helper remembers between starts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Record {
    /// The release this is about; a record about another release is stale.
    pub version: Option<Version>,
    /// Whether it was vouched for.
    pub trust: Trust,
    /// The boots it was started in without being vouched for, oldest first, each once.
    pub boots: Vec<String>,
}

/// What a start should do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Guard {
    /// Start as usual.
    Fine,
    /// The release has used up its boots: restore the previous one first.
    RollBack,
}

impl Record {
    /// A fresh record for a release nobody has vouched for.
    pub fn unconfirmed(version: Version) -> Self {
        Self {
            version: Some(version),
            trust: Trust::Unconfirmed,
            boots: Vec::new(),
        }
    }

    /// A record for a release that is known to work.
    pub fn confirmed(version: Version) -> Self {
        Self {
            version: Some(version),
            trust: Trust::Confirmed,
            boots: Vec::new(),
        }
    }

    /// Notes a start of `current` in boot `boot` and says what to do. A record about another release
    /// is replaced; a confirmed release is always fine; the same boot counts once.
    pub fn started(self, current: &Version, boot: &str, restore: Restore) -> (Self, Guard) {
        let mut record = if self.version.as_ref() == Some(current) {
            self
        } else {
            Self::unconfirmed(current.clone())
        };
        if record.trust == Trust::Confirmed {
            return (record, Guard::Fine);
        }
        if !record.boots.iter().any(|seen| seen == boot) {
            record.boots.push(boot.to_owned());
            let excess = record.boots.len().saturating_sub(KEEP_BOOTS);
            record.boots.drain(..excess);
        }
        let spent = record.boots.len() > MAX_UNCONFIRMED_BOOTS;
        let guard = match restore {
            Restore::Possible if spent => Guard::RollBack,
            Restore::Possible | Restore::Impossible => Guard::Fine,
        };
        (record, guard)
    }
}
