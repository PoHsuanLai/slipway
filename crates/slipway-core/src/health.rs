//! The start-up health decision: did the release that was installed reach a working session?
//!
//! An install leaves a marker saying the new release is awaiting its first login. The session
//! confirms once the shell is up. The daemon starts at the first login (the marker moves on) and at
//! the next start in a different session finds the marker still there if the first login never
//! confirmed. A restart of the daemon inside the session the marker was last set in proves nothing
//! either way, so it changes nothing.

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

/// Which login a start belongs to: the boot and the session on it. A part that could not be read
/// is `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginSession {
    /// The identifier of this boot of the computer.
    pub boot: Option<String>,
    /// The login session on this boot (`XDG_SESSION_ID`), when the session manager set one.
    pub session: Option<String>,
}

impl LoginSession {
    /// Whether `other` is provably the same login: the same known boot and the same session
    /// (neither having one counts as the same). An unknown boot proves nothing.
    pub fn is_same_as(&self, other: &Self) -> bool {
        self.boot.is_some() && self.boot == other.boot && self.session == other.session
    }
}

/// The marker an install leaves behind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    /// The release that was installed.
    pub version: Version,
    /// How far it has got.
    pub stage: PendingStage,
    /// The login the marker was last set in (the one that installed it, then the first login).
    /// Absent in markers written before this was recorded; such a marker is never "the same login".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<LoginSession>,
}

impl Pending {
    /// The marker an install in login `session` leaves.
    pub fn installed(version: Version, session: LoginSession) -> Self {
        Self {
            version,
            stage: PendingStage::AwaitingFirstLogin,
            session: Some(session),
        }
    }

    /// The marker after the first login on the release started, in login `session`.
    pub fn first_login_seen(self, session: LoginSession) -> Self {
        Self {
            stage: PendingStage::FirstLoginSeen,
            session: Some(session),
            ..self
        }
    }

    fn was_set_in(&self, here: &LoginSession) -> bool {
        self.session.as_ref().is_some_and(|s| s.is_same_as(here))
    }
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
    /// The first session has started on the new release: remember that (and which login it was).
    MarkFirstLoginSeen,
    /// This is the login the marker was last set in (the daemon was restarted): wait, change nothing.
    KeepWaiting,
    /// The first session never confirmed: go back to the previous release.
    RollBack,
    /// It never confirmed but there is nothing to go back to: clear the marker.
    GiveUp,
}

/// Decides from the release now current, the marker, the release last confirmed, whether a
/// rollback is possible and the login this start belongs to.
pub fn decide_boot(
    current: Option<&Version>,
    pending: Option<&Pending>,
    confirmed: Option<&Version>,
    restore: Restore,
    here: &LoginSession,
) -> Boot {
    let Some(pending) = pending else {
        return Boot::Nothing;
    };
    if current != Some(&pending.version) || confirmed == current {
        return Boot::Settle;
    }
    if pending.was_set_in(here) {
        return Boot::KeepWaiting;
    }
    match (pending.stage, restore) {
        (PendingStage::AwaitingFirstLogin, _) => Boot::MarkFirstLoginSeen,
        (PendingStage::FirstLoginSeen, Restore::Possible) => Boot::RollBack,
        (PendingStage::FirstLoginSeen, Restore::Impossible) => Boot::GiveUp,
    }
}
