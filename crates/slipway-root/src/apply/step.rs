//! Carries out the steps of a plan, in order, and puts the earlier release back if a step after the
//! switch fails.

use super::error::{ApplyError, Back};
use super::exports::sync_exports;
use super::relabel::{Reach, Relabel, Relabeled};
use super::staged::{Sealed, Staged};
use super::system::sync_system;
use super::unpack::{open_tree, unpack};
use crate::fsutil::{remove_tree, switch_link, sync_dir, sync_tree, write_atomic_mode};
use crate::layout::{Layout, RECORD_DIR, RECORD_RELEASE, RECORD_SIGNATURE};
use semver::Version;
use slipway_core::{Authenticator, Step};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// What running the steps left to tell the person.
#[derive(Debug, Default)]
pub struct Done {
    /// Export links and system files left alone because something else is in the way.
    pub conflicts: Vec<String>,
    /// Things that did not work but did not stop the install (labels that could not be set).
    pub warnings: Vec<String>,
}

/// Whether `current` has been moved yet.
enum Switch {
    /// Not yet: a failure leaves everything as it was.
    Waiting,
    /// Moved to this release: a failure must move it back.
    Made(Version),
}

/// Where `current` and `previous` pointed before the steps ran.
struct Links {
    current: Option<PathBuf>,
    previous: Option<PathBuf>,
}

impl Links {
    fn read(layout: &Layout) -> Self {
        Self {
            current: fs::read_link(layout.current()).ok(),
            previous: fs::read_link(layout.previous()).ok(),
        }
    }

    fn version(target: Option<&Path>) -> Option<Version> {
        Version::parse(target?.file_name()?.to_str()?).ok()
    }

    fn put(link: &Path, target: Option<&Path>) -> io::Result<()> {
        match target {
            Some(target) => switch_link(link, target),
            None => match fs::remove_file(link) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            },
        }
    }

    /// Points both links where they pointed before.
    fn restore(&self, layout: &Layout) -> io::Result<()> {
        Self::put(&layout.current(), self.current.as_deref())?;
        Self::put(&layout.previous(), self.previous.as_deref())
    }
}

/// Runs `steps`. `sealed` is the release being installed, or `None` for a rollback. A new release
/// is relabelled (where the computer uses SELinux) as soon as it is in place, before it becomes current.
///
/// A step that fails after `current` moved (the links or the system files could not be updated) puts
/// the earlier release back, so a failed update never leaves the desktop running a release whose
/// settings are half in place. If even that fails, the error says so.
pub fn run(
    layout: &Layout,
    steps: &[Step],
    sealed: Option<&Sealed>,
    auth: &dyn Authenticator,
    relabel: &dyn Relabel,
) -> Result<Done, ApplyError> {
    let before = Links::read(layout);
    let mut done = Done::default();
    let mut switch = Switch::Waiting;
    match run_steps(layout, steps, sealed, auth, relabel, &mut done, &mut switch) {
        Ok(()) => Ok(done),
        Err(cause) => Err(match switch {
            Switch::Waiting => cause,
            Switch::Made(to) => rewind(layout, &before, &to, auth, relabel, cause),
        }),
    }
}

fn rewind(
    layout: &Layout,
    before: &Links,
    left: &Version,
    auth: &dyn Authenticator,
    relabel: &dyn Relabel,
    cause: ApplyError,
) -> ApplyError {
    let back = match before.restore(layout) {
        Ok(()) => {
            // Best effort: links and system files follow the release that is current again.
            if let Some(earlier) = Links::version(before.current.as_deref()) {
                let _ = sync_exports(layout, auth, Some(left), &earlier);
                let _ = sync_system(layout, auth, Some(left), &earlier, relabel);
            }
            Back::Restored
        }
        Err(_) => Back::NotRestored,
    };
    ApplyError::AfterSwitch {
        cause: Box::new(cause),
        back,
    }
}

fn run_steps(
    layout: &Layout,
    steps: &[Step],
    sealed: Option<&Sealed>,
    auth: &dyn Authenticator,
    relabel: &dyn Relabel,
    done: &mut Done,
    switch: &mut Switch,
) -> Result<(), ApplyError> {
    for step in steps {
        match step {
            Step::Unpack(version) => {
                let sealed =
                    sealed.ok_or_else(|| ApplyError::Unpack("nothing was staged".to_owned()))?;
                let partial = layout.partial_dir(version);
                remove_tree(&partial).map_err(ApplyError::io("clearing an earlier unpack"))?;
                let made = unpack(sealed.bundle(), &sealed.staged.release, &partial)
                    .and_then(|()| keep_record(&partial, &sealed.staged))
                    .and_then(|()| open_tree(&partial));
                if let Err(e) = made {
                    // Best effort: a failed unpack must leave nothing that looks like a release.
                    let _ = remove_tree(&partial);
                    return Err(e);
                }
            }
            Step::Publish(version) => {
                let release = layout.release_dir(version);
                remove_tree(&release).map_err(ApplyError::io("clearing an unfinished release"))?;
                fs::rename(layout.partial_dir(version), &release)
                    .map_err(ApplyError::io("publishing the new release"))?;
                sync_dir(&layout.releases_dir())
                    .map_err(ApplyError::io("flushing the releases folder"))?;
                if let Relabeled::Failed(why) =
                    relabel.relabel(std::slice::from_ref(&layout.root), Reach::Tree)
                {
                    done.warnings.push(format!(
                        "the security labels of the new release could not be set: {why}"
                    ));
                }
            }
            Step::SetPrevious(version) => {
                switch_link(&layout.previous(), &Layout::relative_target(version))
                    .map_err(ApplyError::io("recording the previous release"))?;
            }
            Step::SwitchCurrent(version) => {
                switch_link(&layout.current(), &Layout::relative_target(version))
                    .map_err(ApplyError::io("switching to the new release"))?;
                *switch = Switch::Made(version.clone());
            }
            Step::SyncExports { from, to } => {
                let synced = sync_exports(layout, auth, from.as_ref(), to)?;
                done.conflicts.extend(synced.conflicts);
                done.warnings.extend(synced.warnings);
            }
            Step::SyncSystem { from, to } => {
                let synced = sync_system(layout, auth, from.as_ref(), to, relabel)?;
                done.conflicts.extend(synced.conflicts);
                done.warnings.extend(synced.warnings);
            }
            Step::Prune(doomed) => {
                for version in doomed {
                    remove_tree(&layout.release_dir(version))
                        .map_err(ApplyError::io("removing an old release"))?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Keeps the verified `release.toml` and its signature inside the new release, readable by everyone
/// and writable by root alone.
fn keep_record(partial: &Path, staged: &Staged) -> Result<(), ApplyError> {
    let dir = partial.join(RECORD_DIR);
    write_atomic_mode(&dir.join(RECORD_RELEASE), &staged.release_bytes, 0o644)
        .map_err(ApplyError::io("keeping the release description"))?;
    write_atomic_mode(
        &dir.join(RECORD_SIGNATURE),
        staged.signature.as_bytes(),
        0o644,
    )
    .map_err(ApplyError::io("keeping the signature"))?;
    sync_tree(partial).map_err(ApplyError::io("flushing the new release"))
}
