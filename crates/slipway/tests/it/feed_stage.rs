use crate::private_bus::PrivateBus;
use crate::support::{Accept, Recipe, Reject, Sandbox, build, v};
use peephole_bus::peephole_core::Table;
use semver::Version;
use slipway::config::Config;
use slipway::elevate::{ApplyFailure, Elevate, Recovery};
use slipway::fetch::{
    Download, FeedError, Fetch, FetchError, Patience, Source, StageError, latest, of_version,
    stage, stage_with,
};
use slipway::problem::Problem;
use slipway::report::Report;
use slipway::service::{
    BUS_NAME, Context, Engine, Gate, Job, PATH, Phase, Status, Updates, Wanted, peers_on,
};
use slipway::state::StateStore;
use slipway_core::LoginSession;
use slipway_root::apply::Staged;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Serves bytes from memory and remembers what was asked.
pub struct Memory {
    pub files: BTreeMap<String, Vec<u8>>,
    pub asked: Mutex<Vec<String>>,
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

pub const BASE: &str = "https://example.test/desktop-releases/releases";

pub fn served(
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

/// Tries once and does not wait between tries, so a test of giving up is quick.
const IMPATIENT: Patience = Patience {
    stalls: 1,
    pause: Duration::ZERO,
};

#[test]
fn a_download_that_is_not_the_bundle_is_thrown_away() {
    // Too long, or the right length with the wrong bytes: refused, and nothing is kept.
    let right_length = build(&Recipe::new("0.2.0")).bundle.len();
    for wrong in [vec![0u8; 100_000], vec![0u8; right_length]] {
        let (memory, source, fetched) = served("0.2.0", Some(wrong));
        let dir = tempfile::tempdir().expect("tempdir");
        let err = stage_with(
            &memory,
            &source,
            &fetched,
            dir.path(),
            &mut |_, _| {},
            IMPATIENT,
        )
        .expect_err("mismatch");
        assert!(matches!(err, StageError::Mismatch));
        assert!(!dir.path().join("bundle.tar.zst").exists());
        assert!(!dir.path().join("bundle.tar.zst.part").exists());
    }
}

#[test]
fn a_download_that_ends_short_is_given_up_on_and_never_published() {
    let (memory, source, fetched) = served("0.2.0", Some(b"short".to_vec()));
    let dir = tempfile::tempdir().expect("tempdir");
    let err = stage_with(
        &memory,
        &source,
        &fetched,
        dir.path(),
        &mut |_, _| {},
        IMPATIENT,
    )
    .expect_err("short");
    assert!(
        matches!(err, StageError::Fetch(FetchError::Unreachable(_))),
        "{err}"
    );
    assert!(!dir.path().join("bundle.tar.zst").exists());
}

/// A helper that is never expected to run.
struct NoHelper;

impl Elevate for NoHelper {
    fn install(&self, _: &Path) -> Result<Report, ApplyFailure> {
        Ok(Report::default())
    }

    fn rollback(&self) -> Result<Report, ApplyFailure> {
        Ok(Report::default())
    }

    fn recover(&self) -> Result<Recovery, ApplyFailure> {
        Ok(Recovery::Fine)
    }

    fn confirmed(&self) -> Result<(), ApplyFailure> {
        Ok(())
    }
}

pub(crate) fn engine_serving(sandbox: &Sandbox, version: &str, store: &StateStore) -> Arc<Engine> {
    engine_with(sandbox, version, store, Arc::new(NoHelper))
}

fn engine_with(
    sandbox: &Sandbox,
    version: &str,
    store: &StateStore,
    elevate: Arc<dyn Elevate>,
) -> Arc<Engine> {
    let (memory, source, _) = served(version, None);
    Engine::new(Context {
        config: Config::default(),
        layout: sandbox.layout.clone(),
        cache_dir: sandbox.tmp.path().join("cache"),
        store: store.clone(),
        source,
        fetch: Arc::new(memory),
        anchor: Ok(Arc::new(Accept)),
        elevate,
        login: LoginSession::default(),
        callers: Table::empty(),
        seed: 0,
    })
}

/// Runs one check to its end and returns the status it left.
async fn checked(engine: &Arc<Engine>) -> Status {
    ran(engine, Job::Check).await
}

/// Runs `job` to its end and returns the status it left.
async fn ran(engine: &Arc<Engine>, job: Job) -> Status {
    engine.begin(job).await.expect("the job begins");
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
        assert_eq!(status.available, None);
        assert_eq!(status.current, Some(v("0.1.0")));
        assert!(
            status.last_checked.is_some_and(|at| at.0 > 0),
            "the check itself did happen"
        );

        let newer = engine_serving(&sandbox, "0.3.0", &store);
        let status = checked(&newer).await;
        assert_eq!(status.phase, Phase::Available, "{status:?}");
        assert_eq!(status.available, Some(v("0.3.0")));

        store.update(|s| s.refused = None).expect("forget");
        let status = checked(&engine).await;
        assert_eq!(status.phase, Phase::Available, "{status:?}");
        assert_eq!(status.available, Some(v("0.2.0")));
    });
}

/// A helper that installs for real in the sandbox and then reports a failure, as one does that fails
/// after the switch and cannot switch back.
struct SwitchesThenFails {
    layout: slipway::Layout,
}

impl Elevate for SwitchesThenFails {
    fn install(&self, staged: &Path) -> Result<Report, ApplyFailure> {
        slipway_root::apply::install(&self.layout, staged, &Accept)
            .expect("the install itself works");
        Err(ApplyFailure {
            code: Some(1),
            detail: "a system file could not be placed".into(),
        })
    }

    fn rollback(&self) -> Result<Report, ApplyFailure> {
        Ok(Report::default())
    }

    fn recover(&self) -> Result<Recovery, ApplyFailure> {
        Ok(Recovery::Fine)
    }

    fn confirmed(&self) -> Result<(), ApplyFailure> {
        Ok(())
    }
}

#[test]
fn an_install_that_failed_after_the_switch_is_still_watched_by_the_health_check() {
    let sandbox = Sandbox::new();
    sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    let store = StateStore::new(sandbox.tmp.path().join("state.toml"));
    let helper: Arc<dyn Elevate> = Arc::new(SwitchesThenFails {
        layout: sandbox.layout.clone(),
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let engine = engine_with(&sandbox, "0.2.0", &store, helper);
        let status = ran(&engine, Job::Install(wanting("0.2.0"))).await;
        assert_eq!(status.phase, Phase::Failed, "{status:?}");
        assert_eq!(
            status.current,
            Some(v("0.2.0")),
            "what runs is what the page says"
        );
    });
    assert_eq!(sandbox.current(), Some(v("0.2.0")));
    let pending = store.load().pending.expect("the health marker was written");
    assert_eq!(pending.version, v("0.2.0"));
}

fn wanting(text: &str) -> Wanted {
    Wanted::parse(text).expect("a version")
}

pub(crate) fn block_on<T>(work: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(work)
}

#[test]
fn the_version_the_person_was_shown_is_installed() {
    // Whether the offer was already known or the install looked first.
    for looked_first in [true, false] {
        let sandbox = Sandbox::new();
        sandbox
            .install(&build(&Recipe::new("0.1.0")))
            .expect("install");
        let store = StateStore::new(sandbox.tmp.path().join("state.toml"));
        let engine = engine_serving(&sandbox, "0.2.0", &store);
        let status = block_on(async {
            if looked_first {
                checked(&engine).await;
            }
            ran(&engine, Job::Install(wanting("0.2.0"))).await
        });
        assert_eq!(status.phase, Phase::Installed, "{status:?}");
        assert_eq!(status.current, Some(v("0.2.0")));
    }
}

#[test]
fn a_newer_release_than_the_one_shown_is_refused_before_anything_is_staged() {
    let sandbox = Sandbox::new();
    sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    let store = StateStore::new(sandbox.tmp.path().join("state.toml"));
    let cache = sandbox.tmp.path().join("cache");

    // The offer is known: the call itself is refused and the status is left as it was.
    let engine = engine_serving(&sandbox, "0.3.0", &store);
    block_on(async {
        let known = checked(&engine).await;
        assert_eq!(known.phase, Phase::Available, "{known:?}");
        let refused = engine
            .begin(Job::Install(wanting("0.2.0")))
            .await
            .expect_err("the person saw 0.2.0, 0.3.0 is on offer");
        assert_eq!(refused, Problem::VersionChanged);
        assert_eq!(engine.snapshot(), known, "nothing moved");
    });
    assert!(!cache.join("0.3.0").exists(), "nothing was downloaded");
    assert_eq!(sandbox.current(), Some(v("0.1.0")));

    // The offer is not known yet: the install looks, sees another version, and stops there.
    let fresh = engine_serving(&sandbox, "0.3.0", &store);
    let status = block_on(ran(&fresh, Job::Install(wanting("0.2.0"))));
    assert_eq!(status.phase, Phase::Failed, "{status:?}");
    assert_eq!(status.kind, Some(Problem::VersionChanged));
    assert!(!cache.join("0.3.0").exists(), "nothing was downloaded");
    assert_eq!(sandbox.current(), Some(v("0.1.0")));
}

#[test]
fn naming_a_version_when_nothing_is_on_offer_installs_nothing() {
    let sandbox = Sandbox::new();
    sandbox
        .install(&build(&Recipe::new("0.2.0")))
        .expect("install");
    let store = StateStore::new(sandbox.tmp.path().join("state.toml"));
    let engine = engine_serving(&sandbox, "0.2.0", &store);
    let status = block_on(ran(&engine, Job::Install(wanting("0.3.0"))));
    assert_eq!(status.phase, Phase::Failed, "{status:?}");
    assert_eq!(status.kind, Some(Problem::VersionChanged));
    assert_eq!(sandbox.current(), Some(v("0.2.0")));
}

/// `ProblemKind` as a caller on the bus reads it, never from a cache.
async fn problem_kind_seen_by(client: &zbus::Connection) -> String {
    zbus::proxy::Builder::<zbus::Proxy<'_>>::new(client)
        .destination(BUS_NAME)
        .and_then(|b| b.path(PATH))
        .and_then(|b| b.interface("org.quire.Updates1"))
        .expect("a proxy for the update service")
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .expect("the proxy")
        .get_property::<String>("ProblemKind")
        .await
        .expect("the property")
}

#[test]
fn the_kind_of_a_failure_is_on_the_bus_and_clears_with_the_next_attempt() {
    let sandbox = Sandbox::new();
    sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    let store = StateStore::new(sandbox.tmp.path().join("state.toml"));
    let engine = engine_serving(&sandbox, "0.3.0", &store);
    block_on(async {
        let bus = PrivateBus::start();
        let (server, client) = (bus.connect().await, bus.connect().await);
        let gate = Gate::new(peers_on(server.clone(), Table::empty()));
        server
            .object_server()
            .at(PATH, Updates::new(Arc::clone(&engine), gate))
            .await
            .expect("serve");
        server.request_name(BUS_NAME).await.expect("the name");
        engine.connect(server.clone());

        assert_eq!(
            problem_kind_seen_by(&client).await,
            "",
            "nothing has failed"
        );
        // The person saw 0.2.0 and the offer is not known yet: the install looks, sees 0.3.0 and stops.
        let failed = ran(&engine, Job::Install(wanting("0.2.0"))).await;
        assert_eq!(failed.phase, Phase::Failed, "{failed:?}");
        assert_eq!(problem_kind_seen_by(&client).await, "version-changed");
        let checked = checked(&engine).await;
        assert_eq!(checked.phase, Phase::Available, "{checked:?}");
        assert_eq!(problem_kind_seen_by(&client).await, "");
    });
}
