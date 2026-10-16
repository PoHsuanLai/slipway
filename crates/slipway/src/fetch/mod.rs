//! Getting the release files: where they are, and the download itself.

mod cache;
mod feed;
mod stage;
mod ureq_fetch;

pub use cache::prune_cache;
pub use feed::{FeedError, Fetched, latest, of_version};
pub use stage::{Patience, StageError, stage, stage_with};

pub use ureq_fetch::UreqFetch;

use semver::Version;
use std::io::{self, Read};

/// What a fetch failed with.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// The release site did not answer or the connection broke.
    #[error("could not reach the release site: {0}")]
    Unreachable(String),
    /// The release site answered with an error (a missing release, a server error).
    #[error("the release site answered with an error ({0})")]
    Refused(u16),
    /// More bytes came than the limit.
    #[error("the download is larger than it should be")]
    TooLarge,
    /// A local file could not be written.
    #[error("could not save the download: {0}")]
    Save(#[from] std::io::Error),
}

/// An open download.
pub struct Download {
    /// The size the server announced, if it did.
    pub length: Option<u64>,
    /// The body.
    pub body: Box<dyn Read + Send>,
}

impl std::fmt::Debug for Download {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Download")
            .field("length", &self.length)
            .finish_non_exhaustive()
    }
}

/// Something that can open a URL. The real one is [`UreqFetch`]; tests serve from memory.
pub trait Fetch: Send + Sync {
    /// Opens `url` for reading.
    fn open(&self, url: &str) -> Result<Download, FetchError>;

    /// Opens `url` for reading from byte `offset` on, to resume a download. The default opens it from
    /// the start and throws the first `offset` bytes away, which is right for a server that cannot
    /// resume; one that can overrides it.
    fn open_from(&self, url: &str, offset: u64) -> Result<Download, FetchError> {
        let mut download = self.open(url)?;
        skip(&mut download.body, offset)?;
        Ok(download)
    }
}

/// Reads and drops the next `count` bytes of `reader`; a body that ends sooner is an unreachable site.
pub fn skip(reader: &mut (impl Read + ?Sized), count: u64) -> Result<(), FetchError> {
    let skipped = io::copy(&mut reader.take(count), &mut io::sink())
        .map_err(|e| FetchError::Unreachable(e.to_string()))?;
    if skipped < count {
        return Err(FetchError::Unreachable(
            "the connection ended too early".to_owned(),
        ));
    }
    Ok(())
}

/// Where release files are fetched from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    base: String,
}

impl Source {
    /// A source under the releases page `index_url` (no trailing slash needed).
    pub fn new(index_url: &str) -> Self {
        Self {
            base: index_url.trim_end_matches('/').to_owned(),
        }
    }

    /// The asset `name` of the newest release.
    pub fn latest(&self, name: &str) -> String {
        format!("{}/latest/download/{name}", self.base)
    }

    /// The asset `name` of one version's release (tagged `v<version>`).
    pub fn versioned(&self, version: &Version, name: &str) -> String {
        format!("{}/download/v{version}/{name}", self.base)
    }
}

/// Reads at most `limit` bytes of an open download into memory.
pub fn read_small(download: Download, limit: u64) -> Result<Vec<u8>, FetchError> {
    let mut bytes = Vec::new();
    download.body.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(FetchError::TooLarge);
    }
    Ok(bytes)
}
