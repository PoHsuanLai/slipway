//! The command line: `slipway check|status|install|rollback|confirm`, talking to the daemon.
//!
//! Each command prints one plain line on standard output and exits with a documented code.

mod args;
mod client;
mod confirm;
mod wait;

pub use args::{Command, USAGE, UsageError, parse};
pub use client::{Reply, dispatch};
pub use confirm::confirm;

/// Exit codes of `slipway`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    /// Done (including "already up to date" for `check` and `status`).
    Done = 0,
    /// The thing could not be done; the reason is on standard error.
    Failed = 1,
    /// Wrong command line.
    Usage = 2,
    /// The update service is not running.
    NotRunning = 3,
    /// `install` found nothing newer to install.
    NothingToDo = 4,
}
