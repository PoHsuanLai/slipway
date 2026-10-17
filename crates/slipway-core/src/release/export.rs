//! A link the system needs into the release.

use super::error::ReleaseError;
use crate::path::BundlePath;
use serde::Deserialize;
use std::path::Path;

/// The only places a release may put a link. Each folder is listed by name: there are no wildcards,
/// and a link may sit anywhere below a listed folder but never in a folder beside it. The folder of
/// polkit actions is deliberately not here: an action can let a program run as root with no password
/// for the person at the keyboard, so a release may never add one. The desktop ships none; slipway's own
/// comes from `scripts/install.sh`, and anything else would have to be named, file by file, in the exact
/// list of system files (`system.rs`).
pub const ALLOWED_LINK_DIRS: [&str; 10] = [
    "/usr/share/wayland-sessions",
    "/usr/share/xdg-desktop-portal",
    "/usr/share/dbus-1/services",
    "/usr/lib/systemd/user",
    "/usr/share/applications",
    "/usr/local/bin",
    // Data the desktop's own programs read from the standard data folders.
    "/usr/share/quire",
    "/usr/share/porter",
    "/usr/share/stoker",
    "/usr/share/fonts",
];

/// A link the release wants: `link` is an absolute path in one of [`ALLOWED_LINK_DIRS`]; it points at
/// `<root>/current/<target>`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Export {
    /// Where the link lives on the system.
    pub link: String,
    /// What it points at, relative to the current release.
    pub target: BundlePath,
}

impl Export {
    /// The link as a path.
    pub fn link_path(&self) -> &Path {
        Path::new(&self.link)
    }

    /// Refuses a link outside the allowed places, or one spelled in a way that could escape them.
    pub(super) fn check_link(&self) -> Result<(), ReleaseError> {
        let bad = || ReleaseError::BadExportLink(self.link.clone());
        let parts: Vec<&str> = self.link.split('/').collect();
        let plain = self.link.starts_with('/')
            && parts[1..]
                .iter()
                .all(|p| !p.is_empty() && *p != "." && *p != "..")
            && !self.link.chars().any(|c| c == '\\' || c.is_control());
        let inside = ALLOWED_LINK_DIRS.iter().any(|dir| {
            self.link
                .strip_prefix(dir)
                .is_some_and(|rest| rest.starts_with('/'))
        });
        if plain && inside { Ok(()) } else { Err(bad()) }
    }
}
