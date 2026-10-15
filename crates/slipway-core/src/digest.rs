//! SHA-256 digests as lower-case hex.

use serde::{Deserialize, Deserializer};
use sha2::{Digest, Sha256};
use std::fmt;

/// A SHA-256 digest: exactly 64 lower-case hex digits.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Sha256Hex(String);

/// The text was not 64 lower-case hex digits.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a digest must be 64 lower-case hex digits")]
pub struct DigestError;

impl Sha256Hex {
    /// Parses the hex text; upper case is refused so that one digest has one spelling.
    pub fn parse(text: &str) -> Result<Self, DigestError> {
        let well_formed =
            text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        well_formed
            .then(|| Self(text.to_owned()))
            .ok_or(DigestError)
    }

    /// The digest of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(bytes);
        hasher.finish()
    }

    /// The hex text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Sha256Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sha256Hex({})", self.0)
    }
}

impl fmt::Display for Sha256Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Sha256Hex {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// A running digest, for callers that stream a file through it.
pub struct Hasher(Sha256);

impl fmt::Debug for Hasher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Hasher")
    }
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    /// A new, empty hasher.
    pub fn new() -> Self {
        Self(<Sha256 as Digest>::new())
    }

    /// Feeds more bytes.
    pub fn update(&mut self, bytes: &[u8]) {
        Digest::update(&mut self.0, bytes);
    }

    /// The digest of everything fed so far.
    pub fn finish(self) -> Sha256Hex {
        let out = Digest::finalize(self.0);
        let mut text = String::with_capacity(64);
        for byte in out.iter() {
            text.push_str(&format!("{byte:02x}"));
        }
        Sha256Hex(text)
    }
}
