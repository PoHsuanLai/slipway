//! The configuration: read only, never written by slipway.
//!
//! Two files, one meaning. The system file (`/etc/slipway/config.toml`, written by the installer) holds
//! every setting, including the two that decide what may be installed and where: `root` and
//! `pubkey_path`. The privileged helper reads only that file, and the daemon reads it first, so the two
//! cannot disagree. The person's own file (`$XDG_CONFIG_HOME/slipway/config.toml`) may only change what
//! is shown: `settings_command` and `name`; anything else in it is ignored.
//!
//! The privileged programs read just `root` and `pubkey_path` from the system file
//! (`slipway_root::settings`); the names and defaults of those two are defined there.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

pub use slipway_root::settings::{DEFAULT_PUBKEY_PATH, DEFAULT_ROOT, SYSTEM_CONFIG_PATH};

/// The default release index: the public releases of the desktop.
pub const DEFAULT_INDEX_URL: &str = "https://github.com/PoHsuanLai/desktop-releases/releases";
/// What opens the Software Update page.
pub const DEFAULT_SETTINGS_COMMAND: &str = "detent --page software-update";
/// The name shown for the software.
pub const DEFAULT_NAME: &str = "Quire";
/// Where the privileged helper lives (the polkit action names this path).
pub const DEFAULT_APPLY_PATH: &str = "/usr/libexec/slipway-apply";
/// Where the program that goes back to the previous release lives (it has its own polkit action).
pub const DEFAULT_ROLLBACK_PATH: &str = "/usr/libexec/slipway-rollback";
/// Where the program that keeps the root-side health record lives.
pub const DEFAULT_HEALTH_PATH: &str = "/usr/libexec/slipway-health";
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
    /// The path of the helper that goes back to the previous version.
    pub rollback_path: PathBuf,
    /// The path of the helper that keeps the health record.
    pub health_path: PathBuf,
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
            rollback_path: PathBuf::from(DEFAULT_ROLLBACK_PATH),
            health_path: PathBuf::from(DEFAULT_HEALTH_PATH),
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

/// What a person's own settings file may change.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct UserSettings {
    /// The command a notification's action runs.
    pub settings_command: Option<String>,
    /// The software's name as shown to the person.
    pub name: Option<String>,
}

fn read<T: DeserializeOwned + Default>(path: &Path) -> Result<T, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source,
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(source) => Err(ConfigError::Read {
            path: path.to_owned(),
            source,
        }),
    }
}

impl Config {
    /// Reads `path`; a missing file means all defaults. This is what the privileged helper does with
    /// the system file.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        read(path)
    }

    /// The settings the daemon and the command line use: the system file `system` first, then the
    /// person's file `user` for the two display settings it may change. A blank value is ignored.
    pub fn load_layered(system: &Path, user: &Path) -> Result<Self, ConfigError> {
        let config = Self::load(system)?;
        Ok(config.with_user(read(user)?))
    }

    /// These settings with the person's display choices applied.
    pub fn with_user(self, user: UserSettings) -> Self {
        let filled = |value: Option<String>| value.filter(|v| !v.trim().is_empty());
        Self {
            settings_command: filled(user.settings_command).unwrap_or(self.settings_command),
            name: filled(user.name).unwrap_or(self.name),
            ..self
        }
    }
}
