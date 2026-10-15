//! Paths inside a bundle: relative, plain, and nothing that can climb out of the install tree.

use serde::{Deserialize, Deserializer};
use std::fmt;
use std::path::Path;

/// The directory the updater keeps its own records in, inside each release. A bundle never
/// writes there.
pub const RESERVED_DIR: &str = ".slipway";

const MAX_PATH_BYTES: usize = 1024;
const MAX_COMPONENT_BYTES: usize = 255;

/// Why a bundle path was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PathError {
    /// The path is empty.
    #[error("a path in the release is empty")]
    Empty,
    /// The path begins at the root.
    #[error("a path in the release is absolute: {0}")]
    Absolute(String),
    /// A part of the path is empty, `.` or `..`.
    #[error("a path in the release climbs or doubles up: {0}")]
    Climbing(String),
    /// The path holds a backslash or a control character.
    #[error("a path in the release holds a character that is not allowed: {0}")]
    BadCharacter(String),
    /// The path or one of its parts is too long.
    #[error("a path in the release is too long")]
    TooLong,
    /// The path is inside the updater's own records.
    #[error("a path in the release is inside the updater's own records: {0}")]
    Reserved(String),
}

/// A validated path relative to the release root, with `/` separators.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BundlePath(String);

impl BundlePath {
    /// Validates `text` as a bundle path.
    pub fn parse(text: &str) -> Result<Self, PathError> {
        if text.is_empty() {
            return Err(PathError::Empty);
        }
        if text.len() > MAX_PATH_BYTES {
            return Err(PathError::TooLong);
        }
        if text.starts_with('/') {
            return Err(PathError::Absolute(text.to_owned()));
        }
        if text.chars().any(|c| c == '\\' || c.is_control()) {
            return Err(PathError::BadCharacter(text.to_owned()));
        }
        let parts: Vec<&str> = text.split('/').collect();
        if parts
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == "..")
        {
            return Err(PathError::Climbing(text.to_owned()));
        }
        if parts.iter().any(|p| p.len() > MAX_COMPONENT_BYTES) {
            return Err(PathError::TooLong);
        }
        if parts.first() == Some(&RESERVED_DIR) {
            return Err(PathError::Reserved(text.to_owned()));
        }
        Ok(Self(text.to_owned()))
    }

    /// The path text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The path, to join onto a root.
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }

    /// Every directory above this path, shortest first (`a/b/c` has `a` and `a/b`).
    pub fn ancestors(&self) -> Vec<&str> {
        self.0
            .match_indices('/')
            .map(|(at, _)| &self.0[..at])
            .collect()
    }
}

impl fmt::Debug for BundlePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BundlePath({})", self.0)
    }
}

impl fmt::Display for BundlePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for BundlePath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}
