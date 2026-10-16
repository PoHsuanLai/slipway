//! Downloading the bundle into a staging directory and checking it.
//!
//! A download is resumed, not restarted: the bundle is written as `<name>.part`, and whatever a broken
//! connection (or an earlier run) left there is kept, hashed again and continued from its end with a
//! range request. A connection that stalls is found by the read timeout of the downloader and counts
//! only when it brought no new byte, so a slow link that keeps making progress is never given up on and
//! a dead one is, after a few tries. The digest of the whole file is what decides; resuming trusts
//! nothing.

use super::{Fetch, FetchError, Fetched, Source};
use crate::fsutil::{sync_dir, write_atomic};
use crate::layout::{RECORD_RELEASE, RECORD_SIGNATURE};
use slipway_core::Hasher;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

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

/// How hard a download tries before it gives up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Patience {
    /// How many connections in a row may bring no new byte before the download is abandoned.
    pub stalls: u32,
    /// How long to wait after a connection that brought nothing, times the stalls so far.
    pub pause: Duration,
}

impl Default for Patience {
    fn default() -> Self {
        Self {
            stalls: 4,
            pause: Duration::from_secs(5),
        }
    }
}

/// How one connection ended.
enum Ended {
    /// The whole bundle is here.
    Whole,
    /// The connection ended early; this many new bytes arrived first.
    Early(u64),
}

/// [`stage_with`] with the default patience.
pub fn stage(
    fetch: &dyn Fetch,
    source: &Source,
    fetched: &Fetched,
    dir: &Path,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), StageError> {
    stage_with(fetch, source, fetched, dir, progress, Patience::default())
}

/// Hashes what an earlier run left in `part` and returns how many bytes that was. A file longer than
/// the bundle can be is cleared.
fn keep_earlier(part: &mut File, hasher: &mut Hasher, expected: u64) -> Result<u64, StageError> {
    if part.metadata()?.len() > expected {
        part.set_len(0)?;
    }
    part.seek(SeekFrom::Start(0))?;
    let mut buffer = vec![0u8; 1 << 16];
    let mut done = 0u64;
    loop {
        let n = part.read(&mut buffer)?;
        if n == 0 {
            return Ok(done);
        }
        hasher.update(&buffer[..n]);
        done += n as u64;
    }
}

/// Reads one connection from `done` on, appending to `out`. A read that fails ends the connection
/// (what arrived is kept); a write that fails is an error.
fn one_connection(
    fetch: &dyn Fetch,
    url: &str,
    out: &mut File,
    hasher: &mut Hasher,
    done: &mut u64,
    expected: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Ended, StageError> {
    let started_at = *done;
    let mut download = fetch.open_from(url, started_at)?;
    let mut buffer = vec![0u8; 1 << 16];
    while *done < expected {
        let Ok(n) = download.body.read(&mut buffer) else {
            break;
        };
        if n == 0 {
            break;
        }
        *done += n as u64;
        if *done > expected {
            return Err(StageError::Mismatch);
        }
        hasher.update(&buffer[..n]);
        out.write_all(&buffer[..n])?;
        progress(*done, expected);
    }
    Ok(if *done == expected {
        Ended::Whole
    } else {
        Ended::Early(*done - started_at)
    })
}

/// Saves the verified description and downloads the bundle into `dir`, calling `progress` with the
/// bytes so far and the total expected. The bundle is written as `<name>.part` and renamed once its
/// size and digest match, so a file under its real name is always complete. A `.part` file already
/// there is continued, not started again.
pub fn stage_with(
    fetch: &dyn Fetch,
    source: &Source,
    fetched: &Fetched,
    dir: &Path,
    progress: &mut dyn FnMut(u64, u64),
    patience: Patience,
) -> Result<(), StageError> {
    let release = &fetched.release;
    fs::create_dir_all(dir)?;
    write_atomic(&dir.join(RECORD_RELEASE), &fetched.bytes)?;
    write_atomic(&dir.join(RECORD_SIGNATURE), fetched.signature.as_bytes())?;
    let url = source.versioned(&release.version, &release.bundle.name);
    let part = dir.join(format!("{}.part", release.bundle.name));
    let expected = release.bundle.size;
    let mut out = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(&part)?;
    let mut hasher = Hasher::new();
    let mut done = keep_earlier(&mut out, &mut hasher, expected)?;
    if done > 0 {
        progress(done, expected);
    }
    let mut stalled = 0u32;
    while done < expected {
        let ended = one_connection(
            fetch,
            &url,
            &mut out,
            &mut hasher,
            &mut done,
            expected,
            progress,
        );
        match ended {
            Ok(Ended::Whole) => {}
            Ok(Ended::Early(0)) | Err(StageError::Fetch(FetchError::Unreachable(_))) => {
                stalled += 1;
                if stalled > patience.stalls {
                    return Err(FetchError::Unreachable("the download stalled".to_owned()).into());
                }
                std::thread::sleep(patience.pause * stalled);
            }
            Ok(Ended::Early(_)) => stalled = 0,
            Err(StageError::Fetch(FetchError::Refused(416))) => {
                // The site does not have as many bytes as this file holds: whatever is here is wrong.
                out.set_len(0)?;
                hasher = Hasher::new();
                done = 0;
                stalled += 1;
                if stalled > patience.stalls {
                    return Err(StageError::Mismatch);
                }
            }
            Err(StageError::Mismatch) => {
                let _ = fs::remove_file(&part);
                return Err(StageError::Mismatch);
            }
            Err(other) => return Err(other),
        }
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
