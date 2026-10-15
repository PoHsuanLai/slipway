use crate::support::{Accept, Recipe, Reject, Sandbox, build, v};
use semver::Version;
use slipway::apply::Staged;
use slipway::config::Config;
use slipway::elevate::{ApplyFailure, Elevate};
use slipway::fetch::{
    Download, FeedError, Fetch, FetchError, Source, StageError, latest, of_version, stage,
};
use slipway::service::{Context, Engine, Job, Phase, Status};
use slipway::state::StateStore;
use slipway_core::LoginSession;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

/// A helper that is never expected to run.
struct NoHelper;

impl Elevate for NoHelper {
    fn install(&self, _: &Path) -> Result<(), ApplyFailure> {
        Ok(())
    }

    fn rollback(&self) -> Result<(), ApplyFailure> {
        Ok(())
    }
}

fn engine_serving(sandbox: &Sandbox, version: &str, store: &StateStore) -> Arc<Engine> {
    let (memory, source, _) = served(version, None);
    Engine::new(Context {
        config: Config::default(),
        layout: sandbox.layout.clone(),
        cache_dir: sandbox.tmp.path().join("cache"),
        store: store.clone(),
        source,
        fetch: Arc::new(memory),
        anchor: Ok(Arc::new(Accept)),
        elevate: Arc::new(NoHelper),
        login: LoginSession::default(),
        seed: 0,
    })
}

/// Runs one check to its end and returns the status it left.
async fn checked(engine: &Arc<Engine>) -> Status {
    engine.begin(Job::Check).await.expect("the check begins");
    for _ in 0..500 {
        if !engine.snapshot().phase.is_busy() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    engine.snapshot()
}

#[test]
fn a_rolled_back_version_is_not_offered_again_until_a_newer_one_appears() {
    let sandbox = Sandbox::new();
    sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    let store = StateStore::new(sandbox.tmp.path().join("state.toml"));
    store
        .update(|s| s.refuse(v("0.2.0")))
        .expect("remember the rollback");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let engine = engine_serving(&sandbox, "0.2.0", &store);
        let status = checked(&engine).await;
        assert_eq!(status.phase, Phase::Idle, "{status:?}");
        assert_eq!(status.available, "");
        assert_eq!(status.current, "0.1.0");
        assert!(status.last_checked > 0, "the check itself did happen");

        let newer = engine_serving(&sandbox, "0.3.0", &store);
        let status = checked(&newer).await;
        assert_eq!(status.phase, Phase::Available, "{status:?}");
        assert_eq!(status.available, "0.3.0");

        store.update(|s| s.refused = None).expect("forget");
        let status = checked(&engine).await;
        assert_eq!(status.phase, Phase::Available, "{status:?}");
        assert_eq!(status.available, "0.2.0");
    });
}
