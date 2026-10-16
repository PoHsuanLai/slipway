//! `slipway confirm`: the session says the shell came up, so the release in use is good.
//!
//! The contract the session relies on: run it once per login, after the shell has answered; running it
//! again changes nothing; it needs neither the update service nor the session bus. It records the good
//! start in two places: the person's own state (so the service stops watching the release) and the
//! system's health record (so the release is never rolled back for the boots it used while it was
//! being tried).

use super::{Exit, Reply};
use crate::elevate::Elevate;
use crate::layout::current_version;
use crate::state::StateStore;
use std::path::Path;

/// Records the version `root/current` names as confirmed. Does nothing when no release is
/// installed under the root (a computer not updated by slipway). Returns whether a release is installed.
/// Already confirmed means nothing is written.
pub fn confirm(root: &Path, store: &StateStore) -> std::io::Result<bool> {
    let Some(version) = current_version(root) else {
        return Ok(false);
    };
    if store.load().confirmed.as_ref() != Some(&version) {
        store.update(|s| s.confirmed = Some(version))?;
    }
    Ok(true)
}

/// The whole of `slipway confirm`: the person's state, then the system's record through `elevate`.
pub fn vouch(root: &Path, store: &StateStore, elevate: &dyn Elevate) -> Reply {
    match confirm(root, store) {
        Ok(false) => Reply::done("No update is installed, so there is nothing to confirm."),
        Ok(true) => match elevate.confirmed() {
            Ok(()) => Reply::done("This session started well."),
            Err(failure) => {
                tracing::warn!("could not record the good start: {failure}");
                Reply {
                    exit: Exit::Failed,
                    text: "Could not tell the system that this session started well.".to_owned(),
                }
            }
        },
        Err(e) => {
            tracing::warn!("could not record the good start: {e}");
            Reply {
                exit: Exit::Failed,
                text: "Could not record that this session started well.".to_owned(),
            }
        }
    }
}
