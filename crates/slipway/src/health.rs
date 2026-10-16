//! Start-up health: roll back a release that never reached a working session.
//!
//! The rollback itself is the health helper's decision (`Elevate::recover`), made from a record only root
//! can write; this module decides only when to ask.

use crate::elevate::{Elevate, Recovery};
use crate::layout::Layout;
use crate::state::StateStore;
use slipway_core::{Boot, LoginSession, Restore, decide_boot};

/// What the start-up check did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    /// Nothing to report.
    Fine,
    /// The previous release was restored; tell the person.
    Restored,
}

/// Looks at the marker the last install left and acts on it: moves it on, clears it, or rolls back
/// (through `elevate`) when the first session on the new release never confirmed. `here` is the
/// login this start belongs to: a restart inside the login the marker was last set in changes nothing.
/// A release that is rolled back is remembered as refused, so the next check does not offer it again;
/// so is one whose rollback failed (the marker stays, so the next start tries again).
pub fn on_start(
    layout: &Layout,
    store: &StateStore,
    elevate: &dyn Elevate,
    here: &LoginSession,
) -> Health {
    let Ok(installed) = layout.installed() else {
        return Health::Fine;
    };
    let state = store.load();
    let restore = match (&installed.current, &installed.previous) {
        (Some(c), Some(p)) if c != p && installed.releases.contains(p) => Restore::Possible,
        _ => Restore::Impossible,
    };
    let decision = decide_boot(
        installed.current.as_ref(),
        state.pending.as_ref(),
        state.confirmed.as_ref(),
        restore,
        here,
    );
    match decision {
        Boot::Nothing | Boot::KeepWaiting => Health::Fine,
        Boot::Settle | Boot::GiveUp => {
            store.remember(|s| s.pending = None);
            Health::Fine
        }
        Boot::MarkFirstLoginSeen => {
            store.remember(|s| {
                s.pending = s.pending.take().map(|p| p.first_login_seen(here.clone()));
            });
            Health::Fine
        }
        Boot::RollBack => match elevate.recover() {
            // The helper counts boots itself and may find this release has some left: nothing is undone,
            // the marker stays, and the next start asks again.
            Ok(Recovery::Fine) => Health::Fine,
            Ok(Recovery::Restored(report)) => {
                report.log();
                store.remember(|s| {
                    if let Some(p) = s.pending.take() {
                        s.refuse(p.version);
                    }
                });
                Health::Restored
            }
            Err(failure) => {
                tracing::warn!("the automatic rollback failed: {failure}");
                // The release stays marked so the next start tries again, but it is not offered again.
                store.remember(|s| {
                    let version = s.pending.as_ref().map(|p| p.version.clone());
                    if let Some(version) = version {
                        s.refuse(version);
                    }
                });
                Health::Fine
            }
        },
    }
}
