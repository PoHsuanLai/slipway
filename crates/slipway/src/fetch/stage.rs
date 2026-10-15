//! Downloading the bundle into a staging directory and checking it.

use super::{Fetch, FetchError, Fetched, Source};
use crate::fsutil::{sync_dir, write_atomic};
use crate::layout::{RECORD_RELEASE, RECORD_SIGNATURE};
use slipway_core::Hasher;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

/// Why staging failed.
#[derive(Debug, thiserror::Error)]
pub enum StageError {
    /// The download failed.
    #[error(transparent)]
    Fetch(#[from] FetchError),
    /// The bytes that arrived are not the bundle the release describes.
    #[error("the download does not match the release")]
    Mismatch,
}

impl From<std::io::Error> for StageError {
    fn from(e: std::io::Error) -> Self {
        Self::Fetch(FetchError::Save(e))
    }
}

/// Saves the verified description and downloads the bundle into `dir`, calling `progress` with the
/// bytes so far and the total expected. The bundle is written as `<name>.part` and renamed once its
/// size and digest match, so a file under its real name is always complete.
pub fn stage(
    fetch: &dyn Fetch,
    source: &Source,
    fetched: &Fetched,
    dir: &Path,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), StageError> {
    let release = &fetched.release;
    fs::create_dir_all(dir)?;
    write_atomic(&dir.join(RECORD_RELEASE), &fetched.bytes)?;
    write_atomic(&dir.join(RECORD_SIGNATURE), fetched.signature.as_bytes())?;
    let mut download = fetch.open(&source.versioned(&release.version, &release.bundle.name))?;
    let part = dir.join(format!("{}.part", release.bundle.name));
    let mut out = File::create(&part)?;
    let mut hasher = Hasher::new();
    let mut buffer = vec![0u8; 1 << 16];
    let mut done = 0u64;
    let expected = release.bundle.size;
    loop {
        let n = download.body.read(&mut buffer).map_err(FetchError::Save)?;
        if n == 0 {
            break;
        }
        done += n as u64;
        if done > expected {
            let _ = fs::remove_file(&part);
            return Err(StageError::Mismatch);
        }
        hasher.update(&buffer[..n]);
        out.write_all(&buffer[..n])?;
        progress(done, expected);
    }
    out.sync_all()?;
    if done != expected || hasher.finish() != release.bundle.sha256 {
        let _ = fs::remove_file(&part);
        return Err(StageError::Mismatch);
    }
    fs::rename(&part, dir.join(&release.bundle.name))?;
    sync_dir(dir)?;
    Ok(())
}
