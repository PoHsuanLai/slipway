//! `slipway confirm`: the session says the shell came up, so the release in use is good.

use crate::layout::current_version;
use crate::state::StateStore;
use std::path::Path;

/// Records the version `root/current` names as confirmed. Does nothing when no release is
/// installed under the root (a computer not updated by slipway). Returns whether it recorded one.
pub fn confirm(root: &Path, store: &StateStore) -> std::io::Result<bool> {
    let Some(version) = current_version(root) else {
        return Ok(false);
    };
    store.update(|s| s.confirmed = Some(version))?;
    Ok(true)
}
