//! The privileged side: verify a staged release again from scratch and switch to it atomically.
//!
//! Everything here trusts only the path it is given and the key on disk: the signature, the bundle
//! digest and every file's digest are checked here, whatever the unprivileged side already did.

mod error;
mod exports;
mod install;
mod lock;
mod relabel;
mod staged;
mod step;
mod system;
mod unpack;

pub use error::{ApplyError, ExitCode};
pub use install::{Applied, install, install_with, rollback, rollback_with};
pub use relabel::{NoRelabel, Ran, Reach, Relabel, Relabeled, RunProgram, Selinux, SystemRunner};
pub use staged::{Staged, StagedError};
