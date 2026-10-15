use crate::support::{Accept, Recipe, Reject, build, v};
use semver::Version;
use slipway::apply::Staged;
use slipway::fetch::{
    Download, FeedError, Fetch, FetchError, Source, StageError, latest, of_version, stage,
};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Mutex;

/// Serves bytes from memory and remembers what was asked.
struct Memory {
    files: BTreeMap<String, Vec<u8>>,
    asked: Mutex<Vec<String>>,
}

impl Fetch for Memory {
    fn open(&self, url: &str) -> Result<Download, FetchError> {
        self.asked.lock().expect("lock").push(url.to_owned());
        match self.files.get(url) {
            Some(bytes) => Ok(Download {
                length: Some(bytes.len() as u64),
                body: Box::new(Cursor::new(bytes.clone())),
            }),
            None => Err(FetchError::Refused(404)),
        }
    }
}

const BASE: &str = "https://example.test/desktop-releases/releases";

fn served(
    version: &str,
    bundle_override: Option<Vec<u8>>,
) -> (Memory, Source, slipway::fetch::Fetched) {
    let built = build(&Recipe::new(version));
    let source = Source::new(&format!("{BASE}/"));
    let mut files = BTreeMap::new();
    for url in [
        source.latest("release.toml"),
        source.versioned(&v(version), "release.toml"),
    ] {
        files.insert(url, built.release_toml.clone().into_bytes());
    }
    for url in [
        source.latest("release.toml.minisig"),
        source.versioned(&v(version), "release.toml.minisig"),
    ] {
        files.insert(url, b"signature".to_vec());
    }
    files.insert(
        source.versioned(&v(version), "bundle.tar.zst"),
        bundle_override.unwrap_or(built.bundle),
    );
    let memory = Memory {
        files,
        asked: Mutex::new(Vec::new()),
    };
    let fetched = latest(&memory, &source, &Accept).expect("latest");
    (memory, source, fetched)
}

#[test]
fn urls_follow_the_frozen_layout() {
    let source = Source::new(&format!("{BASE}/"));
    assert_eq!(
        source.latest("release.toml"),
        format!("{BASE}/latest/download/release.toml")
    );
    assert_eq!(
        source.versioned(&v("0.2.0"), "bundle.tar.zst"),
        format!("{BASE}/download/v0.2.0/bundle.tar.zst")
    );
}

#[test]
fn the_newest_release_is_fetched_verified_and_parsed() {
    let (memory, _, fetched) = served("0.2.0", None);
    assert_eq!(fetched.release.version, v("0.2.0"));
    assert_eq!(
        memory.asked.lock().expect("lock").clone(),
        vec![
            format!("{BASE}/latest/download/release.toml"),
            format!("{BASE}/latest/download/release.toml.minisig")
        ]
    );
}

#[test]
fn a_description_with_a_bad_signature_never_gets_as_far_as_parsing() {
    let (memory, source, _) = served("0.2.0", None);
    let err = latest(&memory, &source, &Reject).expect_err("rejected");
    assert!(matches!(err, FeedError::Signature(_)));
}

#[test]
fn no_release_published_is_a_404_the_caller_can_tell_apart() {
    let memory = Memory {
        files: BTreeMap::new(),
        asked: Mutex::new(Vec::new()),
    };
    let err = latest(&memory, &Source::new(BASE), &Accept).expect_err("404");
    assert!(matches!(err, FeedError::Fetch(FetchError::Refused(404))));
}

#[test]
fn a_given_version_is_fetched_from_its_own_tag() {
    let (memory, source, _) = served("0.2.0", None);
    of_version(&memory, &source, &v("0.2.0"), &Accept).expect("versioned");
    assert!(
        memory
            .asked
            .lock()
            .expect("lock")
            .iter()
            .any(|u| u == &format!("{BASE}/download/v0.2.0/release.toml"))
    );
}

#[test]
fn staging_downloads_checks_and_reports_progress() {
    let (memory, source, fetched) = served("0.2.0", None);
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("0.2.0");
    let mut readings: Vec<(u64, u64)> = Vec::new();
    stage(&memory, &source, &fetched, &target, &mut |done, total| {
        readings.push((done, total))
    })
    .expect("stage");
    let last = readings.last().copied().expect("some progress");
    assert_eq!(last.0, last.1);
    assert!(readings.windows(2).all(|w| w[0].0 <= w[1].0));
    assert!(!target.join("bundle.tar.zst.part").exists());
    let staged = Staged::read(&target, &Accept).expect("the staged directory verifies");
    assert_eq!(staged.release.version, Version::new(0, 2, 0));
}

#[test]
fn a_download_that_is_not_the_bundle_is_thrown_away() {
    for wrong in [b"short".to_vec(), vec![0u8; 100_000]] {
        let (memory, source, fetched) = served("0.2.0", Some(wrong));
        let dir = tempfile::tempdir().expect("tempdir");
        let err =
            stage(&memory, &source, &fetched, dir.path(), &mut |_, _| {}).expect_err("mismatch");
        assert!(matches!(err, StageError::Mismatch));
        assert!(!dir.path().join("bundle.tar.zst").exists());
        assert!(!dir.path().join("bundle.tar.zst.part").exists());
    }
}
