//! The configuration file: read only, never written by slipway.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The default install root.
pub const DEFAULT_ROOT: &str = "/opt/quire";
/// The default release index: the public releases of the desktop.
pub const DEFAULT_INDEX_URL: &str = "https://github.com/PoHsuanLai/desktop-releases/releases";
/// Where the trusted public key is installed.
pub const DEFAULT_PUBKEY_PATH: &str = "/usr/share/slipway/release.pub";
/// What opens the Software Update page.
pub const DEFAULT_SETTINGS_COMMAND: &str = "detent --page software-update";
/// The name shown for the software.
pub const DEFAULT_NAME: &str = "Quire";
/// Where the privileged helper lives (the polkit action names this path).
pub const DEFAULT_APPLY_PATH: &str = "/usr/libexec/slipway-apply";
/// The system-wide configuration the privileged helper reads (never the person's own file).
pub const SYSTEM_CONFIG_PATH: &str = "/etc/slipway/config.toml";

/// The settings, each with a default.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Where releases are installed.
    pub root: PathBuf,
    /// The releases page the index and bundles are fetched under.
    pub index_url: String,
    /// The file holding the trusted public key.
    pub pubkey_path: PathBuf,
    /// The command a notification's action runs, split on spaces.
    pub settings_command: String,
    /// The software's name as shown to the person.
    pub name: String,
    /// The privileged helper's path.
    pub apply_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            root: PathBuf::from(DEFAULT_ROOT),
            index_url: DEFAULT_INDEX_URL.to_owned(),
            pubkey_path: PathBuf::from(DEFAULT_PUBKEY_PATH),
            settings_command: DEFAULT_SETTINGS_COMMAND.to_owned(),
            name: DEFAULT_NAME.to_owned(),
            apply_path: PathBuf::from(DEFAULT_APPLY_PATH),
        }
    }
}

/// Why the configuration could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
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
}

impl Config {
    /// Reads `path`; a missing file means all defaults.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|source| ConfigError::Parse {
                path: path.to_owned(),
                source,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }
}
