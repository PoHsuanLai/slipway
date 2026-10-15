//! The ordered steps of an install or a rollback, decided from what is installed.
//!
//! The steps are ordered so that every prefix leaves the system working: the new tree is built
//! beside the old one and only the single rename in `SwitchCurrent` changes what runs.

mod prune;

pub use prune::prune;

use crate::version::{Standing, standing};
use semver::Version;

/// What is on disk under the install root.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Installed {
    /// The release `current` names, if any.
    pub current: Option<Version>,
    /// The release `previous` names, if any.
    pub previous: Option<Version>,
    /// Every complete release directory.
    pub releases: Vec<Version>,
}

/// One step of applying a plan.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Step {
    /// Verify and unpack the bundle into `<version>.partial`, then flush it to disk.
    Unpack(Version),
    /// Rename `<version>.partial` to `<version>`.
    Publish(Version),
    /// Point `previous` at this release.
    SetPrevious(Version),
    /// Point `current` at this release, with one atomic rename.
    SwitchCurrent(Version),
    /// Make the system's links match the exports of `to`, dropping those only `from` had.
    SyncExports {
        /// The release being left, if any.
        from: Option<Version>,
        /// The release now current.
        to: Version,
    },
    /// Delete these release directories.
    Prune(Vec<Version>),
}

/// Why no plan can be made.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PlanError {
    /// The target is the same as or older than what is installed.
    #[error("version {target} is not newer than the installed {current}")]
    NotNewer {
        /// What is installed.
        current: Version,
        /// What was asked for.
        target: Version,
    },
    /// There is nothing installed to roll back from.
    #[error("nothing is installed")]
    NothingInstalled,
    /// There is no earlier release to go back to.
    #[error("there is no earlier version to go back to")]
    NoPrevious,
}

/// The steps that install `target` over `installed`. A target not newer than the current release
/// is refused: a downgrade is only ever a rollback.
pub fn plan_install(installed: &Installed, target: &Version) -> Result<Vec<Step>, PlanError> {
    if let Some(current) = &installed.current
        && standing(current, target) != Standing::Newer
    {
        return Err(PlanError::NotNewer {
            current: current.clone(),
            target: target.clone(),
        });
    }
    let mut steps = vec![Step::Unpack(target.clone()), Step::Publish(target.clone())];
    if let Some(current) = &installed.current {
        steps.push(Step::SetPrevious(current.clone()));
    }
    steps.push(Step::SwitchCurrent(target.clone()));
    steps.push(Step::SyncExports {
        from: installed.current.clone(),
        to: target.clone(),
    });
    let mut releases = installed.releases.clone();
    releases.push(target.clone());
    steps.push(Step::Prune(prune(
        &releases,
        target,
        installed.current.as_ref(),
    )));
    Ok(steps)
}

/// The steps that go back to the previous release. The release being left becomes `previous`, so
/// a rollback can itself be undone.
pub fn plan_rollback(installed: &Installed) -> Result<Vec<Step>, PlanError> {
    let current = installed
        .current
        .as_ref()
        .ok_or(PlanError::NothingInstalled)?;
    let previous = installed
        .previous
        .as_ref()
        .filter(|p| *p != current && installed.releases.contains(p))
        .ok_or(PlanError::NoPrevious)?;
    Ok(vec![
        Step::SetPrevious(current.clone()),
        Step::SwitchCurrent(previous.clone()),
        Step::SyncExports {
            from: Some(current.clone()),
            to: previous.clone(),
        },
        Step::Prune(prune(&installed.releases, previous, Some(current))),
    ])
}
