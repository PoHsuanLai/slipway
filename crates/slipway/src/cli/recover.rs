//! `slipway recover`: run by the session before it starts the compositor.
//!
//! It works without the update service and without the session bus: it asks the privileged health
//! helper, which counts the boots the current release has been started in without being confirmed and
//! puts the previous release back when there were too many. No password is asked: the helper decides
//! from a record only root can write, so a program of the person cannot use it to go back on demand.

use super::{Exit, Reply};
use crate::elevate::{Elevate, Recovery};

/// What the session is told when the previous version was put back.
pub const RESTORED_SESSION_TEXT: &str =
    "The latest update did not start, so the previous version was restored. Starting again.";

/// Asks the health helper to guard this start and turns its answer into the one line to print.
pub fn recover(elevate: &dyn Elevate) -> Reply {
    match elevate.recover() {
        Ok(Recovery::Fine) => Reply::done("Everything is as it should be."),
        Ok(Recovery::Restored(report)) => {
            crate::report::log(&report);
            Reply {
                exit: Exit::Restored,
                text: RESTORED_SESSION_TEXT.to_owned(),
            }
        }
        Err(failure) => {
            tracing::warn!("could not check whether the latest update starts: {failure}");
            Reply::failed("Could not check whether the latest update starts.")
        }
    }
}
