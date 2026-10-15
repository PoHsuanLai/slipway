//! The two operations: install a staged release, or roll back.

use super::error::ApplyError;
use super::lock::Lock;
use super::staged::Staged;
use super::step::run;
use crate::layout::Layout;
use semver::Version;
use slipway_core::{Authenticator, plan_install, plan_rollback};
use std::fs;
use std::path::Path;

/// What an install or rollback did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    /// The release now current.
    pub version: Version,
    /// Export links left alone because something else is in the way.
    pub conflicts: Vec<String>,
}

fn locked(layout: &Layout) -> Result<Lock, ApplyError> {
    fs::create_dir_all(layout.releases_dir())
        .map_err(ApplyError::io("creating the install folder"))?;
    Lock::take(&layout.lock_file())
}

fn read_installed(layout: &Layout) -> Result<slipway_core::Installed, ApplyError> {
    layout
        .installed()
        .map_err(ApplyError::io("looking at what is installed"))
}

/// Verifies the release staged in `staged_dir` from scratch and installs it, switching `current`
/// with one atomic rename. A release not newer than the current one is refused.
pub fn install(
    layout: &Layout,
    staged_dir: &Path,
    auth: &dyn Authenticator,
) -> Result<Applied, ApplyError> {
    let _lock = locked(layout)?;
    let staged = Staged::read(staged_dir, auth)?;
    let version = staged.release.version.clone();
    let steps = plan_install(&read_installed(layout)?, &version)?;
    let conflicts = run(layout, &steps, Some(&staged))?;
    Ok(Applied { version, conflicts })
}

/// Goes back to the previous release; the release left becomes the new `previous`.
pub fn rollback(layout: &Layout) -> Result<Applied, ApplyError> {
    let _lock = locked(layout)?;
    let installed = read_installed(layout)?;
    let steps = plan_rollback(&installed)?;
    let conflicts = run(layout, &steps, None)?;
    let version = read_installed(layout)?
        .current
        .unwrap_or_else(|| Version::new(0, 0, 0));
    Ok(Applied { version, conflicts })
}
