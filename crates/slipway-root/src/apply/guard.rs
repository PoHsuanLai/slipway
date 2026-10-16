//! The root side of the health check: what the helper remembers about a release that was never
//! vouched for, and the one place it rolls back without being asked.
//!
//! The record lives in the private folder, so only root writes it, and counts only the kernel's boot
//! identifier, which only the helper reads for itself (see `slipway_core::guard`). A start in a boot
//! that is not new changes nothing, so a process of the person cannot make a rollback happen without
//! restarting the computer; and a release the session has vouched for is never touched.

use super::error::ApplyError;
use super::install::{Applied, locked, read_installed, rollback_locked};
use super::open::{Capped, read_capped};
use super::relabel::Relabel;
use crate::fsutil::write_atomic_mode;
use crate::layout::Layout;
use slipway_core::{Authenticator, Guard, GuardRecord, Restore};

/// The largest record read: it is a few lines.
const MAX_RECORD_BYTES: usize = 64 * 1024;

/// What a start found.
#[derive(Debug)]
pub enum Started {
    /// Start as usual.
    Fine,
    /// The release in use had used up its boots; the previous one is current now.
    Restored(Applied),
}

fn read_record(layout: &Layout) -> GuardRecord {
    match read_capped(&layout.health_file(), MAX_RECORD_BYTES) {
        Ok(Capped::Bytes(bytes)) => std::str::from_utf8(&bytes)
            .ok()
            .and_then(|text| toml::from_str(text).ok())
            .unwrap_or_default(),
        Ok(Capped::TooLarge) | Err(_) => GuardRecord::default(),
    }
}

/// Writes the record, readable by root alone.
pub(super) fn write_record(layout: &Layout, record: &GuardRecord) -> Result<(), ApplyError> {
    let text = toml::to_string(record).map_err(|e| ApplyError::Unpack(e.to_string()))?;
    write_atomic_mode(&layout.health_file(), text.as_bytes(), 0o600)
        .map_err(ApplyError::io("remembering how the start went"))
}

/// Notes that the session is about to start (or has started) the release that is current, in boot
/// `boot`, and goes back to the previous release when this one has been started in more than
/// [`slipway_core::MAX_UNCONFIRMED_BOOTS`] different boots without ever being confirmed.
pub fn note_start(
    layout: &Layout,
    auth: &dyn Authenticator,
    relabel: &dyn Relabel,
    boot: &str,
) -> Result<Started, ApplyError> {
    if !layout.root.exists() {
        // A computer that is not kept up to date by slipway: nothing to guard, nothing to create.
        return Ok(Started::Fine);
    }
    let _lock = locked(layout)?;
    let installed = read_installed(layout)?;
    let Some(current) = installed.current.clone() else {
        return Ok(Started::Fine);
    };
    let restore = match &installed.previous {
        Some(previous) if *previous != current && installed.releases.contains(previous) => {
            Restore::Possible
        }
        _ => Restore::Impossible,
    };
    let (record, verdict) = read_record(layout).started(&current, boot, restore);
    write_record(layout, &record)?;
    match verdict {
        Guard::Fine => Ok(Started::Fine),
        Guard::RollBack => rollback_locked(layout, auth, relabel).map(Started::Restored),
    }
}

/// Records that the session started well on the release that is current. Does nothing when no release
/// is installed under the root.
pub fn note_confirmed(layout: &Layout) -> Result<(), ApplyError> {
    if !layout.root.exists() {
        return Ok(());
    }
    let _lock = locked(layout)?;
    match read_installed(layout)?.current {
        Some(current) => write_record(layout, &GuardRecord::confirmed(current)),
        None => Ok(()),
    }
}
