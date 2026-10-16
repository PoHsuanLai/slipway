//! The privileged side of slipway: verify a staged release again from scratch, switch to it
//! atomically, go back, and keep the root-side health record.
//!
//! Three small programs (`slipway-apply`, `slipway-rollback`, `slipway-health`) run as root and are
//! built from this crate alone, so they link only `slipway-core`, the archive readers and a few
//! small helpers: no network client, no bus, no async runtime. `scripts/check-boundary.sh` keeps it
//! that way. The unprivileged update service (`slipway`) depends on this crate for the file layout,
//! the report the helper prints and the key loader, so both sides agree on them.
//!
//! The install root and the key path come from the system settings file ([`settings`]), never from
//! the caller.

pub mod apply;
pub mod fsutil;
pub mod helper;
pub mod keys;
pub mod layout;
pub mod report;
pub mod settings;

pub use layout::Layout;
