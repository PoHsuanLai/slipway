//! The two system settings the privileged programs read: where releases are installed and which file
//! holds the key they are checked with.
//!
//! The file (`/etc/slipway/config.toml`, written once by `scripts/install.sh`) may hold more: the update
//! service reads the rest. Here every other key is ignored, so this crate does not need to know the
//! service's settings, and the service reads these two keys with the same names and defaults.

use std::path::{Path, PathBuf};

/// The default install root.
pub const DEFAULT_ROOT: &str = "/opt/quire";
/// Where the trusted public key is installed.
pub const DEFAULT_PUBKEY_PATH: &str = "/usr/share/slipway/release.pub";
/// The system-wide settings file the privileged programs read (never the person's own file).
pub const SYSTEM_CONFIG_PATH: &str = "/etc/slipway/config.toml";

const ROOT_KEY: &str = "root";
const PUBKEY_KEY: &str = "pubkey_path";

/// What the privileged programs take from the system settings, each with a default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootSettings {
    /// Where releases are installed.
    pub root: PathBuf,
    /// The file holding the trusted public key.
    pub pubkey_path: PathBuf,
}

impl Default for RootSettings {
    fn default() -> Self {
        Self {
            root: PathBuf::from(DEFAULT_ROOT),
            pubkey_path: PathBuf::from(DEFAULT_PUBKEY_PATH),
        }
    }
}

/// Why the settings could not be used.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// The file exists but cannot be read.
    #[error("the settings file {path} cannot be read: {source}")]
    Read {
        /// The file.
        path: PathBuf,
        /// Why.
        source: std::io::Error,
    },
    /// The file is not valid settings.
    #[error("the settings file {path} is not valid: {source}")]
    Parse {
        /// The file.
        path: PathBuf,
        /// Why.
        source: toml::de::Error,
    },
    /// A setting that must be a path is something else.
    #[error("the settings file {path} must give {key} as a path in quotes")]
    NotAPath {
        /// The file.
        path: PathBuf,
        /// The setting.
        key: &'static str,
    },
}

impl RootSettings {
    /// Reads `path`; a missing file means all defaults.
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text, path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(SettingsError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }

    /// Reads the text of the settings file; `path` is only named in a refusal.
    pub fn parse(text: &str, path: &Path) -> Result<Self, SettingsError> {
        let table: toml::Table = toml::from_str(text).map_err(|source| SettingsError::Parse {
            path: path.to_owned(),
            source,
        })?;
        let defaults = Self::default();
        let pick = |key: &'static str, default: PathBuf| match table.get(key) {
            None => Ok(default),
            Some(toml::Value::String(text)) => Ok(PathBuf::from(text)),
            Some(_) => Err(SettingsError::NotAPath {
                path: path.to_owned(),
                key,
            }),
        };
        Ok(Self {
            root: pick(ROOT_KEY, defaults.root)?,
            pubkey_path: pick(PUBKEY_KEY, defaults.pubkey_path)?,
        })
    }
}
