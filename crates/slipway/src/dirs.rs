//! Where the daemon keeps its own files, from the XDG variables.

use std::ffi::OsString;
use std::path::PathBuf;

/// The three places slipway reads and writes as a person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dirs {
    /// `$XDG_CONFIG_HOME/slipway/config.toml`.
    pub config_file: PathBuf,
    /// `$XDG_STATE_HOME/slipway/state.toml`.
    pub state_file: PathBuf,
    /// `$XDG_CACHE_HOME/slipway`.
    pub cache_dir: PathBuf,
}

impl Dirs {
    /// Resolves the places from `var`, which looks up an environment variable (so tests need not
    /// touch the real environment). `None` when there is neither the XDG variable nor `HOME`.
    pub fn resolve(var: impl Fn(&str) -> Option<OsString>) -> Option<Self> {
        let under = |xdg: &str, fallback: &str| -> Option<PathBuf> {
            var(xdg)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| var("HOME").map(|h| PathBuf::from(h).join(fallback)))
        };
        Some(Self {
            config_file: under("XDG_CONFIG_HOME", ".config")?.join("slipway/config.toml"),
            state_file: under("XDG_STATE_HOME", ".local/state")?.join("slipway/state.toml"),
            cache_dir: under("XDG_CACHE_HOME", ".cache")?.join("slipway"),
        })
    }
}
