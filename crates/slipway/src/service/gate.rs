//! Who may ask the update service for what, by the program that is calling.
//!
//! The bus names the caller's process; peephole holds it by the descriptor the bus daemon gave, reads
//! what the system says of it and judges it against the desktop's own table of programs (a root-owned
//! file). A method then names the roles it answers:
//!
//! | Method | Answered for |
//! |---|---|
//! | `Install` | the shell, Settings, and the `slipway` program itself |
//! | `Rollback` | the shell and Settings (the system then asks for the administrator's password) |
//! | `Check` | any caller that can be told apart from any other |
//!
//! The shell must be proved as far as the unit manager can (`Proof::Full`). Settings and the `slipway`
//! program are known by their root-owned image (`Proof::Exe`): Settings is also started by a launcher,
//! outside any unit of ours, and the `slipway` program is run from a terminal, so neither has a unit to
//! prove. This is a guard against stray programs, not the boundary for a downgrade: that is the
//! administrator's password.

use crate::problem::Problem;
use peephole_bus::peephole::RealProc;
use peephole_bus::peephole_core::{Proof, Refused, Requirement, Table, roles};
use peephole_bus::{BusPeers, ManagerCheck, Tokio};

/// The bus identification the service uses: the system's `/proc`, probes run off the async threads.
pub type Peers = BusPeers<RealProc, Tokio>;

/// The identification for `connection`, judging against `table`. The unit manager's answers are
/// believed only when the manager is the system's own.
pub fn peers_on(connection: zbus::Connection, table: Table) -> Peers {
    BusPeers::new(
        connection,
        table,
        ManagerCheck::Verified(Requirement::new(roles::MANAGER, Proof::Exe)),
    )
    .with_offload(Tokio)
}

/// Who may start an update.
fn may_install() -> [Requirement; 3] {
    [
        Requirement::new(roles::SHELL, Proof::Full),
        Requirement::new(roles::SETTINGS, Proof::Exe),
        Requirement::new(roles::UPDATER_CLI, Proof::Exe),
    ]
}

/// Who may ask to go back to an earlier version.
fn may_roll_back() -> [Requirement; 2] {
    [
        Requirement::new(roles::SHELL, Proof::Full),
        Requirement::new(roles::SETTINGS, Proof::Exe),
    ]
}

/// The checks the bus object makes before it starts anything.
pub struct Gate {
    peers: Peers,
}

impl std::fmt::Debug for Gate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gate").finish_non_exhaustive()
    }
}

impl Gate {
    /// A gate that identifies callers with `peers`.
    pub fn new(peers: Peers) -> Self {
        Self { peers }
    }

    /// `Ok` when `sender` may start an update; otherwise the sentence to refuse with.
    pub async fn install(&self, sender: Option<&str>) -> Result<(), Problem> {
        self.require(sender, &may_install(), Problem::InstallNotFromHere)
            .await
    }

    /// `Ok` when `sender` may ask to go back; otherwise the sentence to refuse with.
    pub async fn rollback(&self, sender: Option<&str>) -> Result<(), Problem> {
        self.require(sender, &may_roll_back(), Problem::RollbackNotFromHere)
            .await
    }

    /// `Ok` when `sender` can be told apart from any other program on the bus.
    pub async fn check(&self, sender: Option<&str>) -> Result<(), Problem> {
        let Some(sender) = sender else {
            return Err(Problem::CallerUnknown);
        };
        match self.peers.who(sender).await {
            Ok(_) => Ok(()),
            Err(why) => {
                tracing::warn!("a caller of Check could not be identified: {why}");
                Err(Problem::CallerUnknown)
            }
        }
    }

    async fn require(
        &self,
        sender: Option<&str>,
        need: &[Requirement],
        otherwise: Problem,
    ) -> Result<(), Problem> {
        match self.peers.require(sender, need).await {
            Ok(_) => Ok(()),
            Err(why) => Err(refused(sender, why, otherwise)),
        }
    }
}

/// Records a refusal (who asked and why not, never shown to the caller) and gives the sentence.
fn refused(sender: Option<&str>, why: Refused, otherwise: Problem) -> Problem {
    tracing::warn!(
        sender = sender.unwrap_or("none"),
        "a request was refused: {why}"
    );
    otherwise
}
