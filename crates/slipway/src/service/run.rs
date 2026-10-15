//! `slipway run`: start the service and keep it going until told to stop.

use super::context::Context;
use super::engine::{Engine, Job};
use super::notify::{self, RESTORED_TEXT};
use super::object::{BUS_NAME, PATH, Updates};
use crate::clock::now;
use crate::health::{Health, on_start};
use slipway_core::{Attempt, Due, Outcome, next_check};
use std::sync::Arc;
use std::time::Duration;

/// The longest the scheduler sleeps before looking at the clock again.
const LONGEST_SLEEP: Duration = Duration::from_secs(3600);
/// How long to wait after starting a check before asking the schedule again.
const SETTLE: Duration = Duration::from_secs(30);

/// Why the service could not run.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// The session bus is not reachable or the name is taken.
    #[error("the update service could not start: {0}")]
    Bus(#[from] zbus::Error),
    /// Waiting for the stop signal failed.
    #[error("the update service could not listen for a stop signal: {0}")]
    Signal(#[from] std::io::Error),
}

/// Runs the service until interrupted or terminated.
pub async fn run(ctx: Context) -> Result<(), RunError> {
    let health = on_start(&ctx.layout, &ctx.store, &*ctx.elevate);
    let settings_command = ctx.config.settings_command.clone();
    let engine = Engine::new(ctx);
    let connection = zbus::connection::Builder::session()?
        .name(BUS_NAME)?
        .serve_at(PATH, Updates::new(Arc::clone(&engine)))?
        .build()
        .await?;
    engine.connect(connection.clone());
    if health == Health::Restored {
        if let Err(e) = notify::post(&connection, &engine.posted(), RESTORED_TEXT).await {
            tracing::warn!("could not post the restore notice: {e}");
        }
    }
    tokio::spawn(notify::listen(
        connection.clone(),
        engine.posted(),
        settings_command,
    ));
    tokio::spawn(schedule(Arc::clone(&engine)));
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result?,
        _ = terminate.recv() => {},
    }
    Ok(())
}

/// Checks once a day: at start if overdue, then 24 hours plus the jitter after each good check, an
/// hour after a failed one. Each check started is also remembered here as a failed attempt until its
/// own record replaces it, so an unwritable state file cannot make the checks repeat every half minute.
async fn schedule(engine: Arc<Engine>) {
    let mut started: Option<Attempt> = None;
    loop {
        let ctx = engine.context();
        let last = [started, ctx.store.load().last_attempt()]
            .into_iter()
            .flatten()
            .max_by_key(|a| a.at);
        let wait = match next_check(last, now(), ctx.seed) {
            Due::Now => {
                // A busy engine refuses; the answer is only to look again later.
                let _ = engine.begin(Job::Check).await;
                started = Some(Attempt {
                    at: now(),
                    outcome: Outcome::Failed,
                });
                SETTLE
            }
            Due::At(at) => {
                let secs = u64::try_from(at.0.saturating_sub(now().0))
                    .unwrap_or(1)
                    .max(1);
                Duration::from_secs(secs).min(LONGEST_SLEEP)
            }
        };
        tokio::time::sleep(wait).await;
    }
}
