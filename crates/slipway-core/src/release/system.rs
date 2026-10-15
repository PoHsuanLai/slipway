//! A system file the release asks the privileged helper to place.
//!
//! Links (see [`super::Export`]) cover what the desktop's own session reads. A few things only work as a
//! real file owned by root in a system folder: a udev rule, a polkit rule, a login service, the
//! keyboard daemon's unit. The release lists each such file by its exact destination, the file travels
//! in the bundle under `system/` at the same path, and the helper copies it. Nothing else is ever
//! placed: a destination must sit directly in one of [`ALLOWED_SYSTEM_DIRS`].

use super::error::ReleaseError;
use crate::path::BundlePath;
use serde::Deserialize;

/// The folder inside the bundle the system files travel in; their destination follows it.
pub const SYSTEM_TREE: &str = "system";

/// The only folders a release may place a file in, and only directly in them (no sub-folders).
pub const ALLOWED_SYSTEM_DIRS: [&str; 8] = [
    "/etc/udev/rules.d",
    "/etc/modules-load.d",
    "/etc/modprobe.d",
    "/usr/lib/systemd/system",
    "/etc/pam.d",
    "/etc/polkit-1/rules.d",
    "/etc/porter",
    "/etc/quire",
];

/// A file the release places on the system: `path` is its absolute destination in one of
/// [`ALLOWED_SYSTEM_DIRS`]; its contents are the bundle file `system/<path>`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct SystemFile {
    /// Where the file goes on the system.
    pub path: String,
}

fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name.len() <= 255
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+' | b'@'))
}

impl SystemFile {
    /// The bundle path the contents travel at: `system` followed by the destination.
    pub fn source(&self) -> Result<BundlePath, ReleaseError> {
        Ok(BundlePath::parse(&format!("{SYSTEM_TREE}{}", self.path))?)
    }

    /// Refuses a destination that is not directly inside an allowed folder, or is spelled in a way
    /// that could reach another place.
    pub(super) fn check_path(&self) -> Result<(), ReleaseError> {
        let bad = || ReleaseError::BadSystemPath(self.path.clone());
        let (dir, name) = self.path.rsplit_once('/').ok_or_else(bad)?;
        if ALLOWED_SYSTEM_DIRS.contains(&dir) && plain_name(name) {
            Ok(())
        } else {
            Err(bad())
        }
    }
}
