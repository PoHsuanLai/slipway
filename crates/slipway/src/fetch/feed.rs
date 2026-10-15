//! Fetching and checking a signed `release.toml`.

use super::{Fetch, FetchError, Source, read_small};
use semver::Version;
use slipway_core::release::MAX_INDEX_BYTES;
use slipway_core::{Authenticator, Release, ReleaseError, SignError};

const INDEX: &str = "release.toml";
const SIGNATURE: &str = "release.toml.minisig";
const MAX_SIGNATURE_BYTES: u64 = 4096;

/// A release description whose signature has been checked.
#[derive(Clone, Debug)]
pub struct Fetched {
    /// The parsed release.
    pub release: Release,
    /// The signed bytes, exactly as served.
    pub bytes: Vec<u8>,
    /// The signature text.
    pub signature: String,
}

/// Why a release description could not be used.
#[derive(Debug, thiserror::Error)]
pub enum FeedError {
    /// The download failed.
    #[error(transparent)]
    Fetch(#[from] FetchError),
    /// The signature is missing or wrong.
    #[error(transparent)]
    Signature(#[from] SignError),
    /// The signed text is not a valid release.
    #[error(transparent)]
    Release(#[from] ReleaseError),
}

fn get(
    fetch: &dyn Fetch,
    index_url: &str,
    signature_url: &str,
    auth: &dyn Authenticator,
) -> Result<Fetched, FeedError> {
    let bytes = read_small(fetch.open(index_url)?, MAX_INDEX_BYTES as u64)?;
    let signature = String::from_utf8(read_small(fetch.open(signature_url)?, MAX_SIGNATURE_BYTES)?)
        .map_err(|_| SignError::BadSignature)?;
    auth.authenticate(&bytes, &signature)?;
    let release = Release::parse(&bytes)?;
    Ok(Fetched {
        release,
        bytes,
        signature,
    })
}

/// The newest release's description, verified.
pub fn latest(
    fetch: &dyn Fetch,
    source: &Source,
    auth: &dyn Authenticator,
) -> Result<Fetched, FeedError> {
    get(
        fetch,
        &source.latest(INDEX),
        &source.latest(SIGNATURE),
        auth,
    )
}

/// One version's description, verified.
pub fn of_version(
    fetch: &dyn Fetch,
    source: &Source,
    version: &Version,
    auth: &dyn Authenticator,
) -> Result<Fetched, FeedError> {
    get(
        fetch,
        &source.versioned(version, INDEX),
        &source.versioned(version, SIGNATURE),
        auth,
    )
}
