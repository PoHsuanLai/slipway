//! `org.quire.Updates1` on the session bus: the daemon's public face.

use super::engine::{Engine, Job};
use super::gate::Gate;
use super::refusal::InstallError;
use super::wanted::Wanted;
use crate::problem::Problem;
use std::sync::Arc;
use zbus::message::Header;
use zbus::{fdo, interface};

/// The well-known name the daemon owns.
pub const BUS_NAME: &str = "org.quire.Updates1";
/// The object's path.
pub const PATH: &str = "/org/quire/Updates1";

/// The exported object; every property reads the engine's current status.
#[derive(Debug)]
pub struct Updates {
    engine: Arc<Engine>,
    /// Which programs may ask for what.
    gate: Gate,
}

impl Updates {
    /// The object for `engine`, answering the callers `gate` lets through.
    pub fn new(engine: Arc<Engine>, gate: Gate) -> Self {
        Self { engine, gate }
    }

    async fn start(&self, job: Job) -> fdo::Result<()> {
        self.engine
            .begin(job)
            .await
            .map_err(|p| fdo::Error::Failed(p.plain().to_owned()))
    }
}

/// The unique name the call came from, when the message carries one.
fn sender_of<'h>(header: &'h Header<'_>) -> Option<&'h str> {
    header.sender().map(|name| name.as_str())
}

/// The refusal for a caller that may not ask: access denied, with the plain sentence.
fn denied(problem: Problem) -> fdo::Error {
    fdo::Error::AccessDenied(problem.plain().to_owned())
}

#[interface(name = "org.quire.Updates1")]
impl Updates {
    /// Looks for a newer version; returns at once, watch `State`.
    async fn check(&self, #[zbus(header)] header: Header<'_>) -> fdo::Result<()> {
        self.gate.check(sender_of(&header)).await.map_err(denied)?;
        self.start(Job::Check).await
    }

    /// Downloads (if needed), verifies and installs the newer version; returns at once. `version` is
    /// the version the caller showed the person: when the one that would be installed is another
    /// (a newer release arrived meanwhile), nothing starts and the error is
    /// `org.quire.Updates1.Error.VersionChanged`; text that is not a version is `...Error.NotAVersion`.
    async fn install(
        &self,
        #[zbus(header)] header: Header<'_>,
        version: String,
    ) -> Result<(), InstallError> {
        self.gate.install(sender_of(&header)).await?;
        let wanted = Wanted::parse(&version)?;
        self.engine
            .begin(Job::Install(wanted))
            .await
            .map_err(InstallError::from)
    }

    /// Goes back to the previous version; returns at once. Only the shell and Settings may ask (the
    /// system also asks for the administrator's password before it happens).
    async fn rollback(&self, #[zbus(header)] header: Header<'_>) -> fdo::Result<()> {
        self.gate
            .rollback(sender_of(&header))
            .await
            .map_err(denied)?;
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

    /// What kind of failure it was, a stable word for programs and never shown to a person, empty
    /// when nothing went wrong. Changes together with `Problem`.
    #[zbus(property)]
    fn problem_kind(&self) -> String {
        self.engine
            .snapshot()
            .kind
            .map(|kind| kind.kind().to_owned())
            .unwrap_or_default()
    }
}
