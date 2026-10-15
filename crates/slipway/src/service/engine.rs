//! The update flow: check, download, verify, install, roll back; and the status it publishes.

use super::context::Context;
use super::notify::{self, Posted};
use super::object::{PATH, Updates};
use super::status::{Phase, Status};
use crate::apply::Staged;
use crate::clock::now;
use crate::fetch::{self, Fetched};
use crate::fsutil::remove_tree;
use crate::problem::Problem;
use semver::Version;
use slipway_core::{Authenticator, Emit, Offer, Outcome, Pending, ProgressGate, offer};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Instant;
use zbus::Connection;

/// A request to the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Job {
    /// Look for a newer version.
    Check,
    /// Install the newer version (looking first if none is known).
    Install,
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
        let first = match job {
            Job::Check => Phase::Checking,
            Job::Install if lock(&self.offer).is_some() => Phase::Downloading,
            Job::Install => Phase::Checking,
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
            Job::Install => self.install().await,
            Job::Rollback => self.rollback().await,
        };
        if let Err(problem) = result {
            self.change(|s| {
                s.phase = Phase::Failed;
                s.problem = problem.plain().to_owned();
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
    }

    fn authenticator(&self) -> Result<Arc<dyn Authenticator + Send + Sync>, Problem> {
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

    async fn install(self: &Arc<Self>) -> Result<(), Problem> {
        let auth = self.authenticator()?;
        if lock(&self.offer).is_none() {
            self.check().await?;
        }
        // The check above has already settled on Idle when there is nothing newer.
        let Some(fetched) = lock(&self.offer).clone() else {
            return Ok(());
        };
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
        tokio::task::spawn_blocking(move || engine.ctx.elevate.install(&staged))
            .await
            .map_err(|_| Problem::Other)?
            .map_err(|failure| {
                tracing::warn!("installing failed: {failure}");
                Problem::from_apply(&failure)
            })?;
        let pending = Pending::installed(version.clone(), self.ctx.login.clone());
        let _ = self.ctx.store.update(|s| s.pending = Some(pending));
        let _ = remove_tree(&dir);
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

    /// Makes sure `dir` holds the verified release: reuses it if it already verifies, else downloads.
    async fn download(
        self: &Arc<Self>,
        fetched: &Fetched,
        dir: &std::path::Path,
        auth: &Arc<dyn Authenticator + Send + Sync>,
    ) -> Result<(), Problem> {
        let (engine, fetched, dir, auth) = (
            Arc::clone(self),
            fetched.clone(),
            dir.to_owned(),
            Arc::clone(auth),
        );
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
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
        tokio::task::spawn_blocking(move || engine.ctx.elevate.rollback())
            .await
            .map_err(|_| Problem::Other)?
            .map_err(|failure| {
                tracing::warn!("rolling back failed: {failure}");
                match Problem::from_apply(&failure) {
                    Problem::NothingToInstall => Problem::NothingToRestore,
                    other => other,
                }
            })?;
        let _ = self.ctx.store.update(|s| {
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
