//! The wall clock and the per-computer jitter seed; the only readers of either.

use slipway_core::{Sha256Hex, UnixSeconds};
use std::time::{SystemTime, UNIX_EPOCH};

/// The time now.
pub fn now() -> UnixSeconds {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    UnixSeconds(secs)
}

/// A stable number for this computer from its machine id text (any text will do).
pub fn jitter_seed(machine_id: &str) -> u64 {
    let digest = Sha256Hex::of(machine_id.trim().as_bytes());
    u64::from_str_radix(&digest.as_str()[..16], 16).unwrap_or(0)
}
