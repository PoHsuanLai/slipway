//! Which links on the system to create, replace or remove so they match a release's exports.
//!
//! The caller looks at the file system and reports what it found at each link; this module only
//! decides. A link is replaced or removed only when it already points into the install root, and
//! nothing else on the system is ever replaced.

use crate::release::Export;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// What was found at a link's place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// Nothing there.
    Missing,
    /// A symbolic link to this target.
    Link(PathBuf),
    /// A regular file, a directory or anything else that is not a symbolic link.
    Foreign,
}

/// What to do about one link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportAction {
    /// Make a new link.
    Create {
        /// The link's path.
        link: String,
        /// What it points at.
        to: PathBuf,
    },
    /// Re-point a link that already points into the install root.
    Replace {
        /// The link's path.
        link: String,
        /// What it should point at.
        to: PathBuf,
    },
    /// Already right.
    Keep {
        /// The link's path.
        link: String,
    },
    /// Remove a link into the install root that the new release no longer lists.
    Remove {
        /// The link's path.
        link: String,
    },
    /// Something of the person's or the system's is in the way; leave it and tell the person.
    Conflict {
        /// The link's path.
        link: String,
    },
}

/// Whether `target` is a path inside the install root `root`, with no climbing out of it: what a link
/// made by one of our releases' exports points at.
pub fn into_root(target: &Path, root: &Path) -> bool {
    target.starts_with(root) && !target.components().any(|c| c == Component::ParentDir)
}

/// Plans the links for the release `new`, coming from the release whose exports were `old`.
/// `found` says what is at each link of either list (a missing key counts as nothing there).
pub fn plan_exports(
    root: &Path,
    new: &[Export],
    old: &[Export],
    found: &BTreeMap<String, Found>,
) -> Vec<ExportAction> {
    let at = |link: &str| found.get(link).cloned().unwrap_or(Found::Missing);
    let mut actions = Vec::new();
    for export in new {
        let to = root.join("current").join(export.target.as_path());
        let link = export.link.clone();
        actions.push(match at(&link) {
            Found::Missing => ExportAction::Create { link, to },
            Found::Link(t) if t == to => ExportAction::Keep { link },
            Found::Link(t) if into_root(&t, root) => ExportAction::Replace { link, to },
            Found::Link(_) | Found::Foreign => ExportAction::Conflict { link },
        });
    }
    for export in old.iter().filter(|o| !new.iter().any(|n| n.link == o.link)) {
        if let Found::Link(t) = at(&export.link)
            && into_root(&t, root)
        {
            actions.push(ExportAction::Remove {
                link: export.link.clone(),
            });
        }
    }
    actions
}
