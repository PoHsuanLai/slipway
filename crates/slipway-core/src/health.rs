//! The start-up health decision: did the release that was installed reach a working session?
//!
//! An install leaves a marker saying the new release is awaiting its first login. The session
//! confirms once the shell is up. The daemon starts at the first login (the marker moves on) and at
//! the next start finds the marker still there if the first login never confirmed.

use semver::Version;
use serde::{Deserialize, Serialize};

/// How far the new release has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingStage {
    /// Installed; no session has started on it yet.
    AwaitingFirstLogin,
    /// A session started on it (the daemon came up) and has not confirmed yet.
    FirstLoginSeen,
}

/// The marker an install leaves behind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    /// The release that was installed.
    pub version: Version,
    /// How far it has got.
    pub stage: PendingStage,
}

/// Whether an earlier release is there to go back to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Restore {
    /// There is one.
    Possible,
    /// There is none.
    Impossible,
}

/// What the daemon does at start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boot {
    /// Nothing is pending.
    Nothing,
    /// The marker is settled or stale: clear it.
    Settle,
    /// The first session has started on the new release: remember that.
    MarkFirstLoginSeen,
    /// The first session never confirmed: go back to the previous release.
    RollBack,
    /// It never confirmed but there is nothing to go back to: clear the marker.
    GiveUp,
}

/// Decides from the release now current, the marker, the release last confirmed and whether a
/// rollback is possible.
pub fn decide_boot(
    current: Option<&Version>,
    pending: Option<&Pending>,
    confirmed: Option<&Version>,
    restore: Restore,
) -> Boot {
    let Some(pending) = pending else {
        return Boot::Nothing;
    };
    if current != Some(&pending.version) || confirmed == current {
        return Boot::Settle;
    }
    match (pending.stage, restore) {
        (PendingStage::AwaitingFirstLogin, _) => Boot::MarkFirstLoginSeen,
        (PendingStage::FirstLoginSeen, Restore::Possible) => Boot::RollBack,
        (PendingStage::FirstLoginSeen, Restore::Impossible) => Boot::GiveUp,
    }
}
