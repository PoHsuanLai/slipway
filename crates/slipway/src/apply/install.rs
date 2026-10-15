//! The two operations: install a staged release, or roll back.

use super::error::ApplyError;
use super::lock::Lock;
use super::relabel::{NoRelabel, Relabel};
use super::staged::Staged;
use super::step::run;
use crate::layout::Layout;
use semver::Version;
use slipway_core::{Authenticator, plan_install, plan_rollback};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// What an install or rollback did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    /// The release now current.
    pub version: Version,
    /// Export links and system files left alone because something else is in the way.
    pub conflicts: Vec<String>,
    /// Things that did not work but did not stop the install (security labels that could not be set).
    pub warnings: Vec<String>,
}

fn locked(layout: &Layout) -> Result<Lock, ApplyError> {
    // Folders made now are readable by everyone, whatever umask the helper was started with.
    let made: Vec<_> = [layout.root.clone(), layout.releases_dir()]
        .into_iter()
        .filter(|dir| !dir.exists())
        .collect();
    fs::create_dir_all(layout.releases_dir())
        .map_err(ApplyError::io("creating the install folder"))?;
    for dir in made {
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755))
            .map_err(ApplyError::io("opening the install folder"))?;
    }
    Lock::take(&layout.lock_file())
}

fn read_installed(layout: &Layout) -> Result<slipway_core::Installed, ApplyError> {
    layout
        .installed()
        .map_err(ApplyError::io("looking at what is installed"))
}

/// Verifies the release staged in `staged_dir` from scratch and installs it, switching `current`
/// with one atomic rename. A release not newer than the current one is refused. No security labels
/// are set; the helper uses [`install_with`].
pub fn install(
    layout: &Layout,
    staged_dir: &Path,
    auth: &dyn Authenticator,
) -> Result<Applied, ApplyError> {
    install_with(layout, staged_dir, auth, &NoRelabel)
}

/// [`install`], setting the new files' security labels with `relabel` where the computer uses them.
pub fn install_with(
    layout: &Layout,
    staged_dir: &Path,
    auth: &dyn Authenticator,
    relabel: &dyn Relabel,
) -> Result<Applied, ApplyError> {
    let _lock = locked(layout)?;
    let staged = Staged::read(staged_dir, auth)?;
    let version = staged.release.version.clone();
    let steps = plan_install(&read_installed(layout)?, &version)?;
    let done = run(layout, &steps, Some(&staged), relabel)?;
    Ok(Applied {
        version,
        conflicts: done.conflicts,
        warnings: done.warnings,
    })
}

/// Goes back to the previous release; the release left becomes the new `previous`.
pub fn rollback(layout: &Layout) -> Result<Applied, ApplyError> {
    rollback_with(layout, &NoRelabel)
}

/// [`rollback`], setting the labels of any system files put back with `relabel`.
pub fn rollback_with(layout: &Layout, relabel: &dyn Relabel) -> Result<Applied, ApplyError> {
    let _lock = locked(layout)?;
    let installed = read_installed(layout)?;
    let steps = plan_rollback(&installed)?;
    let done = run(layout, &steps, None, relabel)?;
    let version = read_installed(layout)?
        .current
        .unwrap_or_else(|| Version::new(0, 0, 0));
    Ok(Applied {
        version,
        conflicts: done.conflicts,
        warnings: done.warnings,
    })
}
