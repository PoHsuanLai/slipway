//! The install root's layout, and reading what is installed there.
//!
//! ```text
//! <root>/releases/<version>/         complete releases (with .slipway/release.toml inside)
//! <root>/releases/<version>.partial/ an unpack in progress
//! <root>/current  -> releases/<version>
//! <root>/previous -> releases/<version>
//! <root>/lock                        held while installing
//! ```
//!
//! Both the root and the base the export links live under (`/` on a real system) are plain fields,
//! so tests point them at temporary directories.

use semver::Version;
use slipway_core::Installed;
use std::io;
use std::path::{Path, PathBuf};

/// The folder in each release holding the verified description it was installed from.
pub const RECORD_DIR: &str = ".slipway";
/// The verified `release.toml` kept in each release.
pub const RECORD_RELEASE: &str = "release.toml";
/// Its signature.
pub const RECORD_SIGNATURE: &str = "release.toml.minisig";

/// Paths under an install root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    /// The install root (`/opt/quire`).
    pub root: PathBuf,
    /// What an absolute export link path is relative to (`/`).
    pub export_base: PathBuf,
}

impl Layout {
    /// A layout whose export links live in the real file system.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            export_base: PathBuf::from("/"),
        }
    }

    /// The directory holding all releases.
    pub fn releases_dir(&self) -> PathBuf {
        self.root.join("releases")
    }

    /// One release's directory.
    pub fn release_dir(&self, version: &Version) -> PathBuf {
        self.releases_dir().join(version.to_string())
    }

    /// Where a release is unpacked before it is published.
    pub fn partial_dir(&self, version: &Version) -> PathBuf {
        self.releases_dir().join(format!("{version}.partial"))
    }

    /// The `current` symlink.
    pub fn current(&self) -> PathBuf {
        self.root.join("current")
    }

    /// The `previous` symlink.
    pub fn previous(&self) -> PathBuf {
        self.root.join("previous")
    }

    /// The lock file.
    pub fn lock_file(&self) -> PathBuf {
        self.root.join("lock")
    }

    /// The kept `release.toml` of an installed release.
    pub fn record(&self, version: &Version) -> PathBuf {
        self.release_dir(version)
            .join(RECORD_DIR)
            .join(RECORD_RELEASE)
    }

    /// Where an absolute export link path is on this system.
    pub fn export_path(&self, link: &str) -> PathBuf {
        self.export_base.join(link.trim_start_matches('/'))
    }

    /// The symlink target that names a release from `<root>`.
    pub fn relative_target(version: &Version) -> PathBuf {
        PathBuf::from("releases").join(version.to_string())
    }

    /// What is installed: where the links point and which release directories are complete.
    pub fn installed(&self) -> io::Result<Installed> {
        let named = |link: PathBuf| -> Option<Version> {
            let target = std::fs::read_link(link).ok()?;
            Version::parse(target.file_name()?.to_str()?).ok()
        };
        let mut releases = Vec::new();
        match std::fs::read_dir(self.releases_dir()) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry?;
                    let version = entry
                        .file_name()
                        .to_str()
                        .and_then(|n| Version::parse(n).ok());
                    if let Some(v) = version.filter(|_| entry.file_type().is_ok_and(|t| t.is_dir()))
                    {
                        releases.push(v);
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        releases.sort();
        Ok(Installed {
            current: named(self.current()),
            previous: named(self.previous()),
            releases,
        })
    }
}

/// Reads the version `current` names, without the rest.
pub fn current_version(root: &Path) -> Option<Version> {
    Layout::new(root.to_owned()).installed().ok()?.current
}
