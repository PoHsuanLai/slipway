//! A release staged in a directory: `release.toml`, its signature and the bundle, verified.

use crate::layout::{RECORD_RELEASE, RECORD_SIGNATURE};
use slipway_core::release::MAX_INDEX_BYTES;
use slipway_core::{Authenticator, Hasher, Release, ReleaseError, Sha256Hex, SignError};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Why a staged release was refused.
#[derive(Debug, thiserror::Error)]
pub enum StagedError {
    /// A file could not be read.
    #[error("{what}: {source}")]
    Io {
        /// What was being read.
        what: String,
        /// The error.
        source: io::Error,
    },
    /// The signature is wrong.
    #[error(transparent)]
    Signature(#[from] SignError),
    /// The signed description is not a valid release.
    #[error(transparent)]
    Release(#[from] ReleaseError),
    /// The bundle is not the size the release says.
    #[error("the bundle is {found} bytes, not the {expected} the release lists")]
    BundleSize {
        /// What the release says.
        expected: u64,
        /// What the file is.
        found: u64,
    },
    /// The bundle's digest is not the one the release lists.
    #[error("the bundle does not match the release")]
    BundleDigest,
}

/// A release whose signature and bundle digest have been checked.
#[derive(Clone, Debug)]
pub struct Staged {
    /// The parsed release.
    pub release: Release,
    /// The exact signed bytes of `release.toml`.
    pub release_bytes: Vec<u8>,
    /// The signature text.
    pub signature: String,
    /// The bundle file.
    pub bundle: PathBuf,
}

fn io_err(what: &str) -> impl FnOnce(io::Error) -> StagedError + '_ {
    move |source| StagedError::Io {
        what: what.to_owned(),
        source,
    }
}

fn read_limited(path: &Path, limit: usize, what: &str) -> Result<Vec<u8>, StagedError> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|f| f.take(limit as u64 + 1).read_to_end(&mut bytes))
        .map_err(io_err(what))?;
    if bytes.len() > limit {
        return Err(StagedError::Release(ReleaseError::Unreadable(
            "it is too large".to_owned(),
        )));
    }
    Ok(bytes)
}

/// The digest and size of a file, read in a stream.
pub fn digest_file(path: &Path) -> io::Result<(Sha256Hex, u64)> {
    let mut file = File::open(path)?;
    let mut hasher = Hasher::new();
    let mut buffer = vec![0u8; 1 << 16];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            return Ok((hasher.finish(), total));
        }
        hasher.update(&buffer[..n]);
        total += n as u64;
    }
}

impl Staged {
    /// Reads `release.toml`, `release.toml.minisig` and the bundle from `dir`; checks the
    /// signature first, then parses, then checks the bundle's size and digest.
    pub fn read(dir: &Path, auth: &dyn Authenticator) -> Result<Self, StagedError> {
        let release_bytes = read_limited(
            &dir.join(RECORD_RELEASE),
            MAX_INDEX_BYTES,
            "reading the release description",
        )?;
        let signature = String::from_utf8(read_limited(
            &dir.join(RECORD_SIGNATURE),
            4096,
            "reading the signature",
        )?)
        .map_err(|_| StagedError::Signature(SignError::BadSignature))?;
        auth.authenticate(&release_bytes, &signature)?;
        let release = Release::parse(&release_bytes)?;
        let bundle = dir.join(&release.bundle.name);
        let (digest, size) = digest_file(&bundle).map_err(io_err("reading the bundle"))?;
        if size != release.bundle.size {
            return Err(StagedError::BundleSize {
                expected: release.bundle.size,
                found: size,
            });
        }
        if digest != release.bundle.sha256 {
            return Err(StagedError::BundleDigest);
        }
        Ok(Self {
            release,
            release_bytes,
            signature,
            bundle,
        })
    }
}
