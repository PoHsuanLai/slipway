//! The updater: download, verify, install and roll back signed release bundles; the session
//! service that does it once a day on the person's behalf; and the command line.
//!
//! The pure rules live in `slipway-core`. This crate does the I/O and keeps it injectable:
//! the install root and the base the export links live under are plain fields of [`Layout`],
//! downloads go through [`fetch::Fetch`], and the privileged helper through [`elevate::Elevate`],
//! so every test runs in a temporary directory.
//!
//! The three programs that run as root live in `slipway-root`; this crate asks them for work through
//! `pkexec` and never links their install code into the service.

pub mod bootstrap;
pub mod cli;
pub mod clock;
pub mod config;
pub mod dirs;
pub mod elevate;
pub mod fetch;
pub mod health;
pub mod login;
pub mod problem;
pub mod report;
pub mod service;
pub mod state;

// The files and rules the unprivileged side shares with the programs that run as root.
pub use slipway_root::{Layout, fsutil, keys, layout};
