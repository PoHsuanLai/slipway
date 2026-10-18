//! The signature check: minisign, verify only.

use minisign_verify::Signature;

/// Why a signature was not accepted.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SignError {
    /// The key text is not a minisign public key.
    #[error("the release key is not readable")]
    BadKey,
    /// The signature text is not a minisign signature.
    #[error("the release signature is not readable")]
    BadSignature,
    /// The signature does not match the bytes or was made by another key.
    #[error("the release is not signed by the expected key")]
    NotSigned,
}

/// Something that can say whether a message was signed by the trusted key. The real one is
/// [`PublicKey`]; tests supply their own.
pub trait Authenticator: Send + Sync {
    /// `Ok` when `signature` (minisign text) is a valid signature of `message`.
    fn authenticate(&self, message: &[u8], signature: &str) -> Result<(), SignError>;
}

/// The trusted minisign public key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicKey(minisign_verify::PublicKey);

impl PublicKey {
    /// Reads a key from the text of a `.pub` file (comment line, then the key) or from the bare
    /// base64 key on one line.
    pub fn parse(text: &str) -> Result<Self, SignError> {
        let trimmed = text.trim();
        let key = if trimmed.lines().count() >= 2 {
            minisign_verify::PublicKey::decode(trimmed)
        } else {
            minisign_verify::PublicKey::from_base64(trimmed)
        };
        key.map(Self).map_err(|_| SignError::BadKey)
    }
}

impl Authenticator for PublicKey {
    fn authenticate(&self, message: &[u8], signature: &str) -> Result<(), SignError> {
        let signature = Signature::decode(signature).map_err(|_| SignError::BadSignature)?;
        self.0
            .verify(message, &signature, false)
            .map_err(|_| SignError::NotSigned)
    }
}
