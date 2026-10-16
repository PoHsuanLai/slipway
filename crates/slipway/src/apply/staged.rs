//! A release staged in a directory: `release.toml`, its signature and the bundle, verified.
//!
//! Every file is opened once, without following links or waiting on pipes, and read from that handle.
//! The helper's copy of the bundle ([`Staged::seal`]) is taken while it is hashed, so what is unpacked
//! is exactly what the signed digest covers.

use super::open::{Capped, open_regular, read_capped};
use super::private::{free_bytes, open_vault};
use crate::layout::{RECORD_RELEASE, RECORD_SIGNATURE};
use slipway_core::release::MAX_INDEX_BYTES;
use slipway_core::{Authenticator, Hasher, Release, ReleaseError, SignError};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// How many times the bundle's size must be free before it is copied and unpacked: the private copy,
/// and the tree it unpacks to (a compressed tree rarely shrinks to under a third).
const SPACE_FACTOR: u64 = 4;
/// The longest signature text read.
const MAX_SIGNATURE_BYTES: usize = 4096;

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
    #[error("the bundle is not the {expected} bytes the release lists")]
    BundleSize {
        /// What the release says.
        expected: u64,
    },
    /// The bundle's digest is not the one the release lists.
    #[error("the bundle does not match the release")]
    BundleDigest,
    /// There is not room to copy and unpack the bundle.
    #[error("there is not enough free space to install the update")]
    NoSpace,
}

/// A release whose signature has been checked.
#[derive(Clone, Debug)]
pub struct Staged {
    /// The parsed release.
    pub release: Release,
    /// The exact signed bytes of `release.toml`.
    pub release_bytes: Vec<u8>,
    /// The signature text.
    pub signature: String,
    /// The bundle file the caller named.
    pub bundle: PathBuf,
}

/// A staged release whose bundle has been copied into a private file and checked there.
#[derive(Debug)]
pub struct Sealed {
    /// The release, as read.
    pub staged: Staged,
    bundle: File,
}

impl Sealed {
    /// The verified private copy, positioned at its start.
    pub fn bundle(&self) -> &File {
        &self.bundle
    }
}

fn io_err(what: &str) -> impl FnOnce(io::Error) -> StagedError + '_ {
    move |source| StagedError::Io {
        what: what.to_owned(),
        source,
    }
}

fn read_limited(path: &Path, limit: usize, what: &str) -> Result<Vec<u8>, StagedError> {
    match read_capped(path, limit).map_err(io_err(what))? {
        Capped::Bytes(bytes) => Ok(bytes),
        Capped::TooLarge => Err(StagedError::Release(ReleaseError::Unreadable(
            "it is too large".to_owned(),
        ))),
    }
}

/// Reads `source` (at most one byte more than the release lists), hashing it and, when given, writing
/// it to `copy`, then checks the size and the digest.
fn check_stream(
    release: &Release,
    source: File,
    mut copy: Option<&mut File>,
) -> Result<(), StagedError> {
    let expected = release.bundle.size;
    let mut source = source.take(expected.saturating_add(1));
    let mut hasher = Hasher::new();
    let mut buffer = vec![0u8; 1 << 16];
    let mut total = 0u64;
    loop {
        let n = source
            .read(&mut buffer)
            .map_err(io_err("reading the bundle"))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        hasher.update(&buffer[..n]);
        if let Some(out) = copy.as_mut() {
            out.write_all(&buffer[..n])
                .map_err(io_err("keeping a private copy of the bundle"))?;
        }
    }
    if total != expected {
        return Err(StagedError::BundleSize { expected });
    }
    if hasher.finish() != release.bundle.sha256 {
        return Err(StagedError::BundleDigest);
    }
    Ok(())
}

impl Staged {
    /// Reads `release.toml` and `release.toml.minisig` from `dir`; checks the signature first, then
    /// parses. The bundle is not looked at.
    pub fn describe(dir: &Path, auth: &dyn Authenticator) -> Result<Self, StagedError> {
        let release_bytes = read_limited(
            &dir.join(RECORD_RELEASE),
            MAX_INDEX_BYTES,
            "reading the release description",
        )?;
        let signature = String::from_utf8(read_limited(
            &dir.join(RECORD_SIGNATURE),
            MAX_SIGNATURE_BYTES,
            "reading the signature",
        )?)
        .map_err(|_| StagedError::Signature(SignError::BadSignature))?;
        auth.authenticate(&release_bytes, &signature)?;
        let release = Release::parse(&release_bytes)?;
        let bundle = dir.join(&release.bundle.name);
        Ok(Self {
            release,
            release_bytes,
            signature,
            bundle,
        })
    }

    /// [`Staged::describe`], then the bundle's size and digest in place. For the unprivileged side, which
    /// only wants to know whether a download is whole; the helper uses [`Staged::seal`].
    pub fn read(dir: &Path, auth: &dyn Authenticator) -> Result<Self, StagedError> {
        let staged = Self::describe(dir, auth)?;
        let file = open_regular(&staged.bundle).map_err(io_err("reading the bundle"))?;
        check_stream(&staged.release, file, None)?;
        Ok(staged)
    }

    /// Copies the bundle into a private file under `vault_dir` while hashing it, and checks the size and
    /// digest of that one read. Nothing the caller does to the staged file afterwards changes what is
    /// unpacked.
    pub fn seal(self, vault_dir: &Path) -> Result<Sealed, StagedError> {
        let needed = self.release.bundle.size.saturating_mul(SPACE_FACTOR);
        let free = free_bytes(vault_dir).map_err(io_err("looking at the free space"))?;
        if free < needed {
            return Err(StagedError::NoSpace);
        }
        let mut copy = open_vault(vault_dir).map_err(io_err("preparing a private copy"))?;
        let source = open_regular(&self.bundle).map_err(io_err("reading the bundle"))?;
        check_stream(&self.release, source, Some(&mut copy))?;
        copy.seek(SeekFrom::Start(0))
            .map_err(io_err("rewinding the private copy"))?;
        Ok(Sealed {
            staged: self,
            bundle: copy,
        })
    }
}
