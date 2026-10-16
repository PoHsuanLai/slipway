//! `org.quire.Updates1` on the session bus: the daemon's public face.

use super::caller;
use super::engine::{Engine, Job};
use std::path::PathBuf;
use std::sync::Arc;
use zbus::message::Header;
use zbus::{Connection, fdo, interface};

/// The well-known name the daemon owns.
pub const BUS_NAME: &str = "org.quire.Updates1";
/// The object's path.
pub const PATH: &str = "/org/quire/Updates1";

/// The exported object; every property reads the engine's current status.
#[derive(Debug)]
pub struct Updates {
    engine: Arc<Engine>,
    /// Who may ask to go back to an earlier version (from the system settings).
    callers: Vec<String>,
    /// Where processes are looked at (`/proc`).
    proc_root: PathBuf,
}

impl Updates {
    /// The object for `engine`; the callers allowed to roll back come from its settings.
    pub fn new(engine: Arc<Engine>) -> Self {
        let callers = engine.context().config.callers.clone();
        Self {
            engine,
            callers,
            proc_root: PathBuf::from("/proc"),
        }
    }

    async fn start(&self, job: Job) -> fdo::Result<()> {
        self.engine
            .begin(job)
            .await
            .map_err(|p| fdo::Error::Failed(p.plain().to_owned()))
    }
}

#[interface(name = "org.quire.Updates1")]
impl Updates {
    /// Looks for a newer version; returns at once, watch `State`.
    async fn check(&self) -> fdo::Result<()> {
        self.start(Job::Check).await
    }

    /// Downloads (if needed), verifies and installs the newer version; returns at once.
    async fn install(&self) -> fdo::Result<()> {
        self.start(Job::Install).await
    }

    /// Goes back to the previous version; returns at once. Only the shell and Settings may ask (the
    /// system also asks for the administrator's password before it happens).
    async fn rollback(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        caller::check(connection, &header, &self.callers, &self.proc_root).await?;
        self.start(Job::Rollback).await
    }

    /// idle, checking, available, downloading, ready, installing, installed or failed.
    #[zbus(property)]
    fn state(&self) -> String {
        self.engine.snapshot().phase.as_str().to_owned()
    }

    /// The installed version.
    #[zbus(property)]
    fn current(&self) -> String {
        self.engine.snapshot().current
    }

    /// The newer version on offer, empty when none.
    #[zbus(property)]
    fn available(&self) -> String {
        self.engine.snapshot().available
    }

    /// The notes of the version on offer.
    #[zbus(property)]
    fn notes(&self) -> String {
        self.engine.snapshot().notes
    }

    /// The download size in bytes.
    #[zbus(property)]
    fn size(&self) -> u64 {
        self.engine.snapshot().size
    }

    /// Download progress in permille.
    #[zbus(property)]
    fn progress(&self) -> u32 {
        self.engine.snapshot().progress
    }

    /// When the last check finished, unix seconds, 0 for never.
    #[zbus(property)]
    fn last_checked(&self) -> i64 {
        self.engine.snapshot().last_checked
    }

    /// What went wrong in plain words, empty when nothing.
    #[zbus(property)]
    fn problem(&self) -> String {
        self.engine.snapshot().problem
    }
}
