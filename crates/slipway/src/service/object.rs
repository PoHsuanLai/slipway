//! `org.quire.Updates1` on the session bus: the daemon's public face.

use super::engine::{Engine, Job};
use std::sync::Arc;
use zbus::{fdo, interface};

/// The well-known name the daemon owns.
pub const BUS_NAME: &str = "org.quire.Updates1";
/// The object's path.
pub const PATH: &str = "/org/quire/Updates1";

/// The exported object; every property reads the engine's current status.
#[derive(Debug)]
pub struct Updates {
    engine: Arc<Engine>,
}

impl Updates {
    /// The object for `engine`.
    pub fn new(engine: Arc<Engine>) -> Self {
        Self { engine }
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

    /// Goes back to the previous version; returns at once.
    async fn rollback(&self) -> fdo::Result<()> {
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
