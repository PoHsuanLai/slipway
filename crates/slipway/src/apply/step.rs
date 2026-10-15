//! Carries out the steps of a plan, in order.

use super::error::ApplyError;
use super::exports::sync_exports;
use super::relabel::{Reach, Relabel, Relabeled};
use super::staged::Staged;
use super::system::sync_system;
use super::unpack::unpack;
use crate::fsutil::{remove_tree, switch_link, sync_dir, sync_tree, write_atomic};
use crate::layout::{Layout, RECORD_DIR, RECORD_RELEASE, RECORD_SIGNATURE};
use slipway_core::Step;
use std::fs;

/// What running the steps left to tell the person.
#[derive(Debug, Default)]
pub struct Done {
    /// Export links and system files left alone because something else is in the way.
    pub conflicts: Vec<String>,
    /// Things that did not work but did not stop the install (labels that could not be set).
    pub warnings: Vec<String>,
}

/// Runs `steps`. `staged` is the release being installed, or `None` for a rollback. A new release
/// is relabelled (where the computer uses SELinux) as soon as it is in place, before it becomes current.
pub fn run(
    layout: &Layout,
    steps: &[Step],
    staged: Option<&Staged>,
    relabel: &dyn Relabel,
) -> Result<Done, ApplyError> {
    let mut done = Done::default();
    for step in steps {
        match step {
            Step::Unpack(version) => {
                let staged =
                    staged.ok_or_else(|| ApplyError::Unpack("nothing was staged".to_owned()))?;
                let partial = layout.partial_dir(version);
                remove_tree(&partial).map_err(ApplyError::io("clearing an earlier unpack"))?;
                let done = unpack(&staged.bundle, &staged.release, &partial)
                    .and_then(|()| keep_record(&partial, staged));
                if let Err(e) = done {
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
            }
            Step::SyncExports { from, to } => {
                done.conflicts
                    .extend(sync_exports(layout, from.as_ref(), to)?)
            }
            Step::SyncSystem { from, to } => {
                let synced = sync_system(layout, from.as_ref(), to, relabel)?;
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
    Ok(done)
}

/// Keeps the verified `release.toml` and its signature inside the new release.
fn keep_record(partial: &std::path::Path, staged: &Staged) -> Result<(), ApplyError> {
    let dir = partial.join(RECORD_DIR);
    fs::create_dir_all(&dir).map_err(ApplyError::io("creating the release record"))?;
    write_atomic(&dir.join(RECORD_RELEASE), &staged.release_bytes)
        .map_err(ApplyError::io("keeping the release description"))?;
    write_atomic(&dir.join(RECORD_SIGNATURE), staged.signature.as_bytes())
        .map_err(ApplyError::io("keeping the signature"))?;
    sync_tree(partial).map_err(ApplyError::io("flushing the new release"))
}
