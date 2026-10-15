//! A link the system needs into the release.

use super::error::ReleaseError;
use crate::path::BundlePath;
use serde::Deserialize;
use std::path::Path;

/// The only places a release may put a link.
pub const ALLOWED_LINK_DIRS: [&str; 7] = [
    "/usr/share/wayland-sessions",
    "/usr/share/xdg-desktop-portal",
    "/usr/share/dbus-1/services",
    "/usr/lib/systemd/user",
    "/usr/share/applications",
    "/usr/share/polkit-1/actions",
    "/usr/local/bin",
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
