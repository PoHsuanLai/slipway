//! The daemon's own memory: `$XDG_STATE_HOME/slipway/state.toml`, written atomically.

use crate::fsutil::write_atomic;
use semver::Version;
use serde::{Deserialize, Serialize};
use slipway_core::{Attempt, Outcome, Pending, UnixSeconds};
use std::io;
use std::path::PathBuf;

/// What is remembered between runs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UserState {
    /// When the last check ran (unix seconds), if ever.
    pub last_check: Option<UnixSeconds>,
    /// How the last check ended.
    pub last_outcome: Option<Outcome>,
    /// The version the person was last notified about.
    pub notified: Option<Version>,
    /// The version the session last confirmed as working.
    pub confirmed: Option<Version>,
    /// The install that has not yet been confirmed.
    pub pending: Option<Pending>,
    /// The newest version that was rolled back; it and anything older is not offered again.
    pub refused: Option<Version>,
}

impl UserState {
    /// Remembers that `version` was rolled back, keeping the newer of it and what was refused before.
    pub fn refuse(&mut self, version: Version) {
        let newest = match self.refused.take() {
            Some(before) if before > version => before,
            _ => version,
        };
        self.refused = Some(newest);
    }

    /// The last check as the schedule wants it.
    pub fn last_attempt(&self) -> Option<Attempt> {
        Some(Attempt {
            at: self.last_check?,
            outcome: self.last_outcome?,
        })
    }
}

/// The state file.
#[derive(Clone, Debug)]
pub struct StateStore {
    path: PathBuf,
}

impl StateStore {
    /// A store at `path`.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Reads the state; a missing or unreadable file is an empty state (it is only a memory).
    pub fn load(&self) -> UserState {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// [`StateStore::update`] for a change nobody can act on if it fails: the failure is logged, not
    /// dropped, so a marker that did not get written leaves a trace.
    pub fn remember(&self, change: impl FnOnce(&mut UserState)) {
        if let Err(e) = self.update(change) {
            tracing::warn!("could not write {}: {e}", self.path.display());
        }
    }

    /// Reads, changes and writes the state in one go.
    pub fn update(&self, change: impl FnOnce(&mut UserState)) -> io::Result<UserState> {
        let mut state = self.load();
        change(&mut state);
        let text = toml::to_string(&state).map_err(io::Error::other)?;
        write_atomic(&self.path, text.as_bytes())?;
        Ok(state)
    }
}
