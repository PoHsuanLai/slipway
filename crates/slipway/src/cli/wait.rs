//! Waiting for the daemon to finish what it was asked to do.

use super::client::UpdatesClientProxy;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(250);

/// Which states end the wait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Until {
    /// A check has finished: anything but "checking".
    CheckDone,
    /// An install or rollback has finished: installed, idle or failed.
    WorkDone,
}

impl Until {
    fn reached(self, state: &str) -> bool {
        match self {
            Self::CheckDone => state != "checking",
            Self::WorkDone => matches!(state, "installed" | "idle" | "failed"),
        }
    }
}

/// Polls `State` until `until` is reached, calling `note` when the state changes. `None` when
/// `limit` passes first.
pub async fn wait(
    proxy: &UpdatesClientProxy<'_>,
    until: Until,
    limit: Duration,
    mut note: impl FnMut(&str),
) -> zbus::Result<Option<String>> {
    let started = Instant::now();
    let mut last = String::new();
    while started.elapsed() < limit {
        let state = proxy.state().await?;
        if state != last {
            note(&state);
            last.clone_from(&state);
        }
        if until.reached(&state) {
            return Ok(Some(state));
        }
        tokio::time::sleep(POLL).await;
    }
    Ok(None)
}
