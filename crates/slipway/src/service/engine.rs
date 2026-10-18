//! The update flow: check, download, verify, install, roll back; and the status it publishes.

use super::context::Context;
use super::notify::{self, Posted};
use super::object::{PATH, Updates};
use super::status::{Phase, Status};
use super::wanted::Wanted;
use crate::clock::now;
use crate::fetch::{self, Fetched};
use crate::fsutil::remove_tree;
use crate::problem::Problem;
use semver::Version;
use slipway_core::{Authenticator, Emit, Offer, Outcome, Pending, ProgressGate, offer};
use slipway_root::apply::Staged;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Instant;
use zbus::Connection;

/// A request to the engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Job {
    /// Look for a newer version.
    Check,
    /// Install the newer version (looking first if none is known), provided it is the one the
    /// person was shown.
    Install(Wanted),
    /// Go back to the previous version.
    Rollback,
}

/// The update flow and its published status.
pub struct Engine {
    ctx: Context,
    status: Mutex<Status>,
    offer: Mutex<Option<Fetched>>,
    bus: OnceLock<Connection>,
    posted: Posted,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("status", &self.snapshot())
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Engine {
    /// An engine whose status starts from what is installed and when the last check was.
    pub fn new(ctx: Context) -> Arc<Self> {
        let current = ctx
            .layout
            .installed()
            .ok()
            .and_then(|i| i.current)
            .map(|v| v.to_string())
            .unwrap_or_default();
        let last = ctx.store.load().last_check.map_or(0, |t| t.0);
        Arc::new(Self {
            status: Mutex::new(Status::starting(current, last)),
            offer: Mutex::new(None),
            bus: OnceLock::new(),
            posted: Posted::default(),
            ctx,
        })
    }

    /// Gives the engine the bus connection its status is announced on.
    pub fn connect(&self, connection: Connection) {
        let _ = self.bus.set(connection);
    }

    /// The notifications this engine has posted.
    pub fn posted(&self) -> Posted {
        self.posted.clone()
    }

    /// The collaborators.
    pub fn context(&self) -> &Context {
        &self.ctx
    }

    /// The status now.
    pub fn snapshot(&self) -> Status {
        lock(&self.status).clone()
    }

    /// Starts `job` in the background. Refused while other work is in progress.
    pub async fn begin(self: &Arc<Self>, job: Job) -> Result<(), Problem> {
        let first = match &job {
            Job::Check => Phase::Checking,
            Job::Install(wanted) => match lock(&self.offer).as_ref() {
                // A different version is on offer than the one shown: refuse now, touching nothing.
                Some(fetched) => {
                    wanted.accepts(&fetched.release.version)?;
                    Phase::Downloading
                }
                None => Phase::Checking,
            },
            Job::Rollback => Phase::Installing,
        };
        let old = {
            let mut status = lock(&self.status);
            if status.phase.is_busy() {
                return Err(Problem::Busy);
            }
            let old = status.clone();
            status.phase = first;
            status.problem.clear();
            status.kind = None;
            status.progress = 0;
            old
        };
        self.announce(&old).await;
        tokio::spawn(Arc::clone(self).run(job));
        Ok(())
    }

    async fn run(self: Arc<Self>, job: Job) {
        let result = match job {
            Job::Check => self.check().await,
            Job::Install(wanted) => self.install(&wanted).await,
            Job::Rollback => self.rollback().await,
        };
        if let Err(problem) = result {
            self.change(|s| {
                s.phase = Phase::Failed;
                s.problem = problem.plain().to_owned();
                s.kind = Some(problem);
                s.progress = 0;
            })
            .await;
        }
    }

    async fn change(&self, edit: impl FnOnce(&mut Status)) {
        let old = {
            let mut status = lock(&self.status);
            let old = status.clone();
            edit(&mut status);
            old
        };
        self.announce(&old).await;
    }

    /// Emits a change signal for each property that differs from `old`.
    async fn announce(&self, old: &Status) {
        let Some(bus) = self.bus.get() else { return };
        let Ok(iface_ref) = bus.object_server().interface::<_, Updates>(PATH).await else {
            return;
        };
        let new = self.snapshot();
        let iface = iface_ref.get().await;
        let emitter = iface_ref.signal_emitter();
        if old.phase != new.phase {
            let _ = iface.state_changed(emitter).await;
        }
        if old.current != new.current {
            let _ = iface.current_changed(emitter).await;
        }
        if old.available != new.available {
            let _ = iface.available_changed(emitter).await;
        }
        if old.notes != new.notes {
            let _ = iface.notes_changed(emitter).await;
        }
        if old.size != new.size {
            let _ = iface.size_changed(emitter).await;
        }
        if old.progress != new.progress {
            let _ = iface.progress_changed(emitter).await;
        }
        if old.last_checked != new.last_checked {
            let _ = iface.last_checked_changed(emitter).await;
        }
        if old.problem != new.problem {
            let _ = iface.problem_changed(emitter).await;
        }
        if old.kind != new.kind {
            let _ = iface.problem_kind_changed(emitter).await;
        }
    }

    fn authenticator(&self) -> Result<Arc<dyn Authenticator>, Problem> {
        match &self.ctx.anchor {
            Ok(auth) => Ok(Arc::clone(auth)),
            Err(e) => Err(Problem::from(e)),
        }
    }

    fn installed_version(&self) -> Option<Version> {
        self.ctx.layout.installed().ok().and_then(|i| i.current)
    }

    fn record_check(&self, outcome: Outcome) {
        let at = now();
        let _ = self.ctx.store.update(|s| {
            s.last_check = Some(at);
            s.last_outcome = Some(outcome);
        });
    }

    async fn check(self: &Arc<Self>) -> Result<(), Problem> {
        let auth = self.authenticator()?;
        let (engine, source) = (Arc::clone(self), self.ctx.source.clone());
        let fetched =
            tokio::task::spawn_blocking(move || fetch::latest(&*engine.ctx.fetch, &source, &*auth))
                .await
                .map_err(|_| Problem::Other)?;
        match fetched {
            Ok(fetched) => {
                self.record_check(Outcome::Succeeded);
                self.found(fetched).await;
                Ok(())
            }
            Err(fetch::FeedError::Fetch(crate::fetch::FetchError::Refused(404))) => {
                // No release has been published yet: nothing newer than what is installed.
                self.record_check(Outcome::Succeeded);
                self.found_nothing().await;
                Ok(())
            }
            Err(e) => {
                tracing::warn!("checking for updates failed: {e}");
                self.record_check(Outcome::Failed);
                Err(Problem::from(&e))
            }
        }
    }

    async fn found_nothing(&self) {
        let checked = now().0;
        *lock(&self.offer) = None;
        self.change(|s| {
            s.phase = Phase::Idle;
            s.available.clear();
            s.notes.clear();
            s.size = 0;
            s.last_checked = checked;
        })
        .await;
    }

    async fn found(&self, fetched: Fetched) {
        let current = self
            .installed_version()
            .unwrap_or_else(|| Version::new(0, 0, 0));
        let refused = self.ctx.store.load().refused;
        if offer(&current, &fetched.release.version, refused.as_ref()) != Offer::Present {
            self.found_nothing().await;
            return;
        }
        let release = fetched.release.clone();
        *lock(&self.offer) = Some(fetched);
        let checked = now().0;
        self.change(|s| {
            s.phase = Phase::Available;
            s.available = release.version.to_string();
            s.notes = release.notes.clone();
            s.size = release.bundle.size;
            s.last_checked = checked;
        })
        .await;
        self.notify_once(&release.version).await;
    }

    /// Posts a notification with `text`, if the bus is there.
    async fn tell(&self, text: &str) {
        let Some(bus) = self.bus.get() else { return };
        if let Err(e) = notify::post(bus, &self.posted, text).await {
            tracing::warn!("could not post the notification: {e}");
        }
    }

    /// One notification per version, ever.
    async fn notify_once(&self, version: &Version) {
        if self.ctx.store.load().notified.as_ref() == Some(version) {
            return;
        }
        let Some(bus) = self.bus.get() else { return };
        let body = notify::available_text(&self.ctx.config.name, version);
        match notify::post(bus, &self.posted, &body).await {
            Ok(_) => {
                let _ = self
                    .ctx
                    .store
                    .update(|s| s.notified = Some(version.clone()));
            }
            Err(e) => tracing::warn!("could not post the notification: {e}"),
        }
    }

    async fn install(self: &Arc<Self>, wanted: &Wanted) -> Result<(), Problem> {
        let auth = self.authenticator()?;
        if lock(&self.offer).is_none() {
            self.check().await?;
        }
        // Nothing newer on offer after the check: not the version the person was shown either.
        let Some(fetched) = lock(&self.offer).clone() else {
            return Err(Problem::VersionChanged);
        };
        wanted.accepts(&fetched.release.version)?;
        let version = fetched.release.version.clone();
        let dir = self.ctx.cache_dir.join(version.to_string());
        self.change(|s| {
            s.phase = Phase::Downloading;
            s.progress = 0;
        })
        .await;
        self.download(&fetched, &dir, &auth).await?;
        self.change(|s| {
            s.phase = Phase::Ready;
            s.progress = 1000;
        })
        .await;
        self.change(|s| s.phase = Phase::Installing).await;
        let (engine, staged) = (Arc::clone(self), dir.clone());
        let outcome = tokio::task::spawn_blocking(move || engine.ctx.elevate.install(&staged))
            .await
            .map_err(|_| Problem::Other)?;
        let report = match outcome {
            Ok(report) => report,
            Err(failure) => {
                tracing::warn!("installing failed: {failure}");
                // A helper that failed after it switched, and could not switch back, leaves this release
                // current: it must still be watched, or it would run unmonitored.
                if self.installed_version().as_ref() == Some(&version) {
                    self.remember_install(&version);
                    self.change(|s| s.current = version.to_string()).await;
                }
                return Err(Problem::from_apply(&failure));
            }
        };
        crate::report::log(&report);
        self.remember_install(&version);
        let _ = remove_tree(&dir);
        if !report.conflicts.is_empty() {
            self.tell(notify::LEFT_ALONE_TEXT).await;
        }
        *lock(&self.offer) = None;
        self.change(|s| {
            s.phase = Phase::Installed;
            s.current = version.to_string();
            s.available.clear();
            s.notes.clear();
            s.size = 0;
            s.progress = 0;
        })
        .await;
        Ok(())
    }

    /// Leaves the marker the health check looks for: `version` was installed in this login.
    fn remember_install(&self, version: &Version) {
        let pending = Pending::installed(version.clone(), self.ctx.login.clone());
        self.ctx.store.remember(|s| s.pending = Some(pending));
    }

    /// Makes sure `dir` holds the verified release: reuses it if it already verifies, else downloads.
    async fn download(
        self: &Arc<Self>,
        fetched: &Fetched,
        dir: &std::path::Path,
        auth: &Arc<dyn Authenticator>,
    ) -> Result<(), Problem> {
        let (engine, fetched, dir, auth) = (
            Arc::clone(self),
            fetched.clone(),
            dir.to_owned(),
            Arc::clone(auth),
        );
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            // Earlier downloads that were superseded or never finished are not worth keeping.
            if let Err(e) = fetch::prune_cache(&engine.ctx.cache_dir, &fetched.release.version) {
                tracing::warn!("could not clear the old downloads: {e}");
            }
            if Staged::read(&dir, &*auth).is_ok() {
                return Ok(());
            }
            let mut gate = ProgressGate::new();
            let started = Instant::now();
            let mut report = |done: u64, total: u64| {
                let permille =
                    u32::try_from(done.saturating_mul(1000) / total.max(1)).unwrap_or(1000);
                let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                if gate.admit(permille, ms) == Emit::Announce {
                    runtime.block_on(engine.change(|s| s.progress = permille));
                }
            };
            fetch::stage(
                &*engine.ctx.fetch,
                &engine.ctx.source,
                &fetched,
                &dir,
                &mut report,
            )
            .map_err(|e| {
                tracing::warn!("downloading failed: {e}");
                Problem::from(&e)
            })?;
            Staged::read(&dir, &*auth).map(drop).map_err(|e| {
                tracing::warn!("the download did not verify: {e}");
                Problem::Unverified
            })
        })
        .await
        .map_err(|_| Problem::Other)?
    }

    async fn rollback(self: &Arc<Self>) -> Result<(), Problem> {
        let left = self.installed_version();
        let engine = Arc::clone(self);
        let report = tokio::task::spawn_blocking(move || engine.ctx.elevate.rollback())
            .await
            .map_err(|_| Problem::Other)?
            .map_err(|failure| {
                tracing::warn!("rolling back failed: {failure}");
                match Problem::from_apply(&failure) {
                    Problem::NothingToInstall => Problem::NothingToRestore,
                    other => other,
                }
            })?;
        crate::report::log(&report);
        if !report.conflicts.is_empty() {
            self.tell(notify::LEFT_ALONE_TEXT).await;
        }
        self.ctx.store.remember(|s| {
            s.pending = None;
            if let Some(left) = left {
                s.refuse(left);
            }
        });
        let current = self
            .installed_version()
            .map(|v| v.to_string())
            .unwrap_or_default();
        self.change(|s| {
            s.phase = Phase::Installed;
            s.current = current;
        })
        .await;
        Ok(())
    }
}
