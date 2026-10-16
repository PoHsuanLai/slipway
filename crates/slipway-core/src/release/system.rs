//! A system file the release asks the privileged helper to place.
//!
//! Links (see [`super::Export`]) cover what the desktop's own session reads. A few things only work as a
//! real file owned by root in a system folder: a udev rule, a polkit rule, a login service, the
//! keyboard daemon's unit. The release lists each such file by its exact destination, the file travels
//! in the bundle under `system/` at the same path, and the helper copies it.
//!
//! The policy is per file, not per folder: a destination is accepted only when [`ALLOWED_SYSTEM_FILES`]
//! names it. A folder alone would let a compromised build step add `/etc/polkit-1/rules.d/00-all.rules`
//! to a signed release; with this list that release is refused whole. The cost is deliberate: the helper
//! is installed by hand, not by a release, so a new system file needs a new helper.

use super::error::ReleaseError;
use crate::path::BundlePath;
use serde::Deserialize;

/// The folder inside the bundle the system files travel in; their destination follows it.
pub const SYSTEM_TREE: &str = "system";

/// Which names are accepted in one folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameRule {
    /// Exactly this name.
    Exact(&'static str),
    /// Any plain name ending in this text (the machine-wide tables, one per daemon).
    Ending(&'static str),
}

impl NameRule {
    fn accepts(self, name: &str) -> bool {
        match self {
            Self::Exact(only) => name == only,
            Self::Ending(tail) => name.len() > tail.len() && name.ends_with(tail),
        }
    }
}

/// One folder and what may be placed directly in it (no sub-folders).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemRule {
    /// The folder, absolute, without a trailing slash.
    pub dir: &'static str,
    /// The names accepted in it.
    pub name: NameRule,
}

const fn rule(dir: &'static str, name: NameRule) -> SystemRule {
    SystemRule { dir, name }
}

/// Every file a release may place, and nothing else.
pub const ALLOWED_SYSTEM_FILES: [SystemRule; 13] = [
    // The shell's login service.
    rule("/etc/pam.d", NameRule::Exact("sill")),
    // Brightness over the monitor's own bus.
    rule("/etc/udev/rules.d", NameRule::Exact("60-sill-ddc.rules")),
    rule("/etc/modules-load.d", NameRule::Exact("sill-ddc.conf")),
    // The keyboard daemon: its unit (in the vendor folder, /etc stays the administrator's), its device
    // rule, the rule that lets the person restart it and the modules it needs.
    rule("/usr/lib/systemd/system", NameRule::Exact("keycap.service")),
    rule("/etc/udev/rules.d", NameRule::Exact("70-keycap.rules")),
    rule(
        "/etc/polkit-1/rules.d",
        NameRule::Exact("keycap.polkit.rules"),
    ),
    rule("/etc/modules-load.d", NameRule::Exact("keycap.conf")),
    // The palm-rejection daemon's device rule and module options.
    rule("/etc/udev/rules.d", NameRule::Exact("70-palmrest.rules")),
    rule("/etc/modprobe.d", NameRule::Exact("99-palmrest.conf")),
    // Who may call each daemon: the machine's caller tables.
    rule("/etc/porter", NameRule::Exact("callers.toml")),
    rule("/etc/porter", NameRule::Exact("inferd-callers.toml")),
    rule("/etc/quire", NameRule::Ending("-callers.toml")),
    // The desktop's system-wide default settings (the XDG config folder, read below the person's own).
    rule("/etc/xdg/quire", NameRule::Ending(".toml")),
];

/// A file the release places on the system: `path` is its absolute destination, one of
/// [`ALLOWED_SYSTEM_FILES`]; its contents are the bundle file `system/<path>`.
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

/// Whether `path` is a file a release may place: directly in a listed folder, spelled plainly, and
/// accepted by that folder's rule.
pub fn is_allowed_system_path(path: &str) -> bool {
    let Some((dir, name)) = path.rsplit_once('/') else {
        return false;
    };
    plain_name(name)
        && ALLOWED_SYSTEM_FILES
            .iter()
            .any(|rule| rule.dir == dir && rule.name.accepts(name))
}

impl SystemFile {
    /// The bundle path the contents travel at: `system` followed by the destination.
    pub fn source(&self) -> Result<BundlePath, ReleaseError> {
        Ok(BundlePath::parse(&format!("{SYSTEM_TREE}{}", self.path))?)
    }

    /// Refuses a destination that no rule names, or that is spelled in a way that could reach
    /// another place.
    pub(super) fn check_path(&self) -> Result<(), ReleaseError> {
        if is_allowed_system_path(&self.path) {
            Ok(())
        } else {
            Err(ReleaseError::BadSystemPath(self.path.clone()))
        }
    }
}
