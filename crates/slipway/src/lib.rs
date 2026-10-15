//! The updater: download, verify, install and roll back signed release bundles; the session
//! service that does it once a day on the person's behalf; and the command line.
//!
//! The pure rules live in `slipway-core`. This crate does the I/O and keeps it injectable:
//! the install root and the base the export links live under are plain fields of [`Layout`],
//! downloads go through [`fetch::Fetch`], and the privileged helper through [`elevate::Elevate`],
//! so every test runs in a temporary directory.

pub mod apply;
pub mod bootstrap;
pub mod cli;
pub mod clock;
pub mod config;
pub mod dirs;
pub mod elevate;
pub mod fetch;
pub mod fsutil;
pub mod health;
pub mod keys;
pub mod layout;
pub mod problem;
pub mod service;
pub mod state;

pub use layout::Layout;
