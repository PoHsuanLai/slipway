//! Start-up health: roll back a release that never reached a working session.

use crate::elevate::Elevate;
use crate::layout::Layout;
use crate::state::StateStore;
use slipway_core::{Boot, PendingStage, Restore, decide_boot};

/// What the start-up check did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    /// Nothing to report.
    Fine,
    /// The previous release was restored; tell the person.
    Restored,
}

/// Looks at the marker the last install left and acts on it: moves it on, clears it, or rolls back
/// (through `elevate`) when the first session on the new release never confirmed.
pub fn on_start(layout: &Layout, store: &StateStore, elevate: &dyn Elevate) -> Health {
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
    );
    match decision {
        Boot::Nothing => Health::Fine,
        Boot::Settle | Boot::GiveUp => {
            let _ = store.update(|s| s.pending = None);
            Health::Fine
        }
        Boot::MarkFirstLoginSeen => {
            let _ = store.update(|s| {
                if let Some(p) = s.pending.as_mut() {
                    p.stage = PendingStage::FirstLoginSeen;
                }
            });
            Health::Fine
        }
        Boot::RollBack => match elevate.rollback() {
            Ok(()) => {
                let _ = store.update(|s| s.pending = None);
                Health::Restored
            }
            Err(failure) => {
                tracing::warn!("the automatic rollback failed: {failure}");
                Health::Fine
            }
        },
    }
}
