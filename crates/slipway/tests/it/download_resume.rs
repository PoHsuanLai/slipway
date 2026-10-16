//! A download that breaks is continued, a connection that brings nothing is given up on, and old
//! downloads do not pile up in the cache.

use crate::feed_stage::{Memory, served};
use crate::support::{Accept, v};
use slipway::apply::Staged;
use slipway::fetch::{
    Download, Fetch, FetchError, Patience, Source, StageError, prune_cache, stage_with,
};
use std::io::{self, Cursor, Read};
use std::sync::Mutex;
use std::time::Duration;

/// Serves like a site that honours byte ranges, but each connection dies after the number of bytes
/// listed for it (the last listed number repeats; `None` means it never dies). Remembers the offsets asked for.
struct Dropping {
    inner: Memory,
    cuts: Mutex<Vec<Option<usize>>>,
    offsets: Mutex<Vec<u64>>,
}

/// A body that fails with a timeout after `left` bytes.
struct Cut {
    bytes: Cursor<Vec<u8>>,
    left: Option<usize>,
}

impl Read for Cut {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let room = self.left.map_or(buf.len(), |left| left.min(buf.len()));
        if room == 0 {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        let n = self.bytes.read(&mut buf[..room])?;
        if let Some(left) = self.left.as_mut() {
            *left -= n;
        }
        Ok(n)
    }
}

impl Fetch for Dropping {
    fn open(&self, url: &str) -> Result<Download, FetchError> {
        self.open_from(url, 0)
    }

    fn open_from(&self, url: &str, offset: u64) -> Result<Download, FetchError> {
        let full = self.inner.open(url)?;
        let mut all = Vec::new();
        full.body.take(u64::MAX).read_to_end(&mut all)?;
        if url.ends_with("bundle.tar.zst") {
            self.offsets.lock().expect("lock").push(offset);
        }
        let mut cuts = self.cuts.lock().expect("lock");
        let left = if cuts.len() > 1 {
            cuts.remove(0)
        } else {
            cuts[0]
        };
        let from = usize::try_from(offset).expect("offset");
        Ok(Download {
            length: Some((all.len() - from) as u64),
            body: Box::new(Cut {
                bytes: Cursor::new(all[from..].to_vec()),
                left: if url.ends_with("bundle.tar.zst") {
                    left
                } else {
                    None
                },
            }),
        })
    }
}

/// A site whose connections die after the bytes `cuts` gives for the bundle's size.
fn dropping(
    cuts: impl FnOnce(usize) -> Vec<Option<usize>>,
) -> (Dropping, Source, slipway::fetch::Fetched) {
    let (memory, source, fetched) = served("0.2.0", None);
    let dropping = Dropping {
        inner: memory,
        cuts: Mutex::new(cuts(
            usize::try_from(fetched.release.bundle.size).expect("size"),
        )),
        offsets: Mutex::new(Vec::new()),
    };
    (dropping, source, fetched)
}

const QUICK: Patience = Patience {
    stalls: 2,
    pause: Duration::ZERO,
};

#[test]
fn a_broken_connection_is_continued_from_where_it_ended() {
    let (site, source, fetched) = dropping(|total| vec![Some(total / 3), Some(total / 4), None]);
    let total = fetched.release.bundle.size;
    let (first, second) = (total / 3, total / 4);
    let dir = tempfile::tempdir().expect("tempdir");
    let mut readings = Vec::new();
    stage_with(
        &site,
        &source,
        &fetched,
        dir.path(),
        &mut |done, _| readings.push(done),
        QUICK,
    )
    .expect("stage");
    assert_eq!(
        *site.offsets.lock().expect("lock"),
        vec![0, first, first + second]
    );
    assert_eq!(readings.last().copied(), Some(total));
    assert!(readings.windows(2).all(|w| w[0] <= w[1]));
    Staged::read(dir.path(), &Accept).expect("the result verifies");
    assert!(!dir.path().join("bundle.tar.zst.part").exists());
}

#[test]
fn a_connection_that_brings_nothing_is_tried_a_few_times_then_given_up() {
    let (site, source, fetched) = dropping(|_| vec![Some(0)]);
    let dir = tempfile::tempdir().expect("tempdir");
    let err = stage_with(&site, &source, &fetched, dir.path(), &mut |_, _| {}, QUICK)
        .expect_err("stalled");
    assert!(
        matches!(err, StageError::Fetch(FetchError::Unreachable(_))),
        "{err}"
    );
    assert_eq!(
        site.offsets.lock().expect("lock").len(),
        3,
        "the first try and two more"
    );
    assert!(!dir.path().join("bundle.tar.zst").exists());
}

#[test]
fn a_slow_link_that_keeps_making_progress_is_never_given_up_on() {
    // Every connection brings a few bytes and dies: far more tries than the stall allowance.
    let (site, source, fetched) = dropping(|total| vec![Some((total / 8).max(1))]);
    let dir = tempfile::tempdir().expect("tempdir");
    stage_with(&site, &source, &fetched, dir.path(), &mut |_, _| {}, QUICK).expect("stage");
    assert!(site.offsets.lock().expect("lock").len() >= 8);
    Staged::read(dir.path(), &Accept).expect("the result verifies");
}

#[test]
fn what_an_earlier_run_left_is_continued_and_a_wrong_leftover_is_caught_by_the_digest() {
    let (memory, source, fetched) = served("0.2.0", None);
    let whole = memory
        .files
        .get(&source.versioned(&v("0.2.0"), "bundle.tar.zst"))
        .expect("the bundle")
        .clone();

    // The first half of the real bundle: the rest is fetched, the result verifies.
    let half = whole.len() / 2;
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("bundle.tar.zst.part"), &whole[..half]).expect("part");
    let mut first = None;
    stage_with(
        &memory,
        &source,
        &fetched,
        dir.path(),
        &mut |done, _| {
            first.get_or_insert(done);
        },
        QUICK,
    )
    .expect("continued");
    assert_eq!(
        first,
        Some(half as u64),
        "progress starts where the leftover ends"
    );
    Staged::read(dir.path(), &Accept).expect("verifies");

    // Leftover bytes that are not the bundle's: everything arrives, the digest refuses, nothing stays.
    let other = tempfile::tempdir().expect("tempdir");
    std::fs::write(other.path().join("bundle.tar.zst.part"), vec![7u8; half]).expect("part");
    let err = stage_with(
        &memory,
        &source,
        &fetched,
        other.path(),
        &mut |_, _| {},
        QUICK,
    )
    .expect_err("wrong leftover");
    assert!(matches!(err, StageError::Mismatch));
    assert!(!other.path().join("bundle.tar.zst.part").exists());
    assert!(!other.path().join("bundle.tar.zst").exists());

    // A leftover longer than the bundle can be is cleared and the download starts again.
    let long = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        long.path().join("bundle.tar.zst.part"),
        vec![7u8; 1_000_000],
    )
    .expect("part");
    stage_with(
        &memory,
        &source,
        &fetched,
        long.path(),
        &mut |_, _| {},
        QUICK,
    )
    .expect("restarted");
    Staged::read(long.path(), &Accept).expect("verifies");
}

#[test]
fn only_the_download_being_fetched_stays_in_the_cache() {
    let cache = tempfile::tempdir().expect("tempdir");
    for name in ["0.1.0", "0.2.0", "0.3.0", "not-a-version"] {
        std::fs::create_dir_all(cache.path().join(name).join("inside")).expect("dir");
    }
    std::fs::write(cache.path().join("0.4.0"), "a file, not a download").expect("file");
    prune_cache(cache.path(), &v("0.3.0")).expect("prune");
    let mut left: Vec<String> = std::fs::read_dir(cache.path())
        .expect("read")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, vec!["0.3.0", "0.4.0", "not-a-version"]);
    prune_cache(&cache.path().join("missing"), &v("0.3.0")).expect("a missing cache is fine");
}
