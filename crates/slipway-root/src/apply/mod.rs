//! The privileged side: verify a staged release again from scratch and switch to it atomically.
//!
//! Everything here trusts only the path it is given and the key on disk: the signature, the bundle
//! digest and every file's digest are checked here, whatever the unprivileged side already did. Files
//! the caller names are opened once and read from that handle (`open`); the bundle is copied into a
//! private file while it is hashed (`staged`, `private`); the lock and that copy live in a folder only
//! root can open (`lock`); and the process's own state is reset before anything else (`harden`).

mod error;
mod exports;
mod guard;
pub mod harden;
mod install;
mod lock;
pub mod open;
mod private;
mod relabel;
mod settle;
mod staged;
mod step;
mod synced;
mod system;
mod unpack;

pub use error::{ApplyError, Back, ExitCode};
pub use guard::{Started, note_confirmed, note_start};
pub use install::{Applied, install, install_with, rollback, rollback_with};
pub use relabel::{NoRelabel, Ran, Reach, Relabel, Relabeled, RunProgram, Selinux, SystemRunner};
pub use settle::{Change, Command, Settling, commands};
pub use staged::{Sealed, Staged, StagedError};
