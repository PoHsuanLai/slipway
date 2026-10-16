use crate::support::{Accept, Recipe, Sandbox, build, v};
use slipway::elevate::{ApplyFailure, Elevate};
use slipway::health::{Health, on_start};
use slipway::report::Report;
use slipway::state::StateStore;
use slipway_core::{LoginSession, Pending, PendingStage};
use std::path::Path;
use std::sync::Mutex;

/// Stands in for the privileged helper: records the calls and rolls back in the sandbox.
struct Recording<'a> {
    sandbox: &'a Sandbox,
    calls: Mutex<Vec<&'static str>>,
}

impl Elevate for Recording<'_> {
    fn install(&self, _: &Path) -> Result<Report, ApplyFailure> {
        self.calls.lock().expect("lock").push("install");
        Ok(Report::default())
    }

    fn rollback(&self) -> Result<Report, ApplyFailure> {
        self.calls.lock().expect("lock").push("rollback");
        slipway::apply::rollback(&self.sandbox.layout, &Accept)
            .map(|applied| Report::from(&applied))
            .map_err(|e| ApplyFailure {
                code: Some(e.exit_code() as i32),
                detail: e.to_string(),
            })
    }
}

fn two_releases() -> (Sandbox, StateStore) {
    let sandbox = Sandbox::new();
    sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    sandbox
        .install(&build(&Recipe::new("0.2.0")))
        .expect("install");
    let store = StateStore::new(sandbox.tmp.path().join("state/state.toml"));
    (sandbox, store)
}

/// The login a start belongs to: boot `b1` and the numbered session.
fn login(session: &str) -> LoginSession {
    LoginSession {
        boot: Some("b1".to_owned()),
        session: Some(session.to_owned()),
    }
}

/// The marker an install made in session 1 leaves, moved on to `stage`.
fn mark(store: &StateStore, stage: PendingStage) {
    let installed = Pending::installed(v("0.2.0"), login("1"));
    let marker = match stage {
        PendingStage::AwaitingFirstLogin => installed,
        PendingStage::FirstLoginSeen => installed.first_login_seen(login("2")),
    };
    store.update(|s| s.pending = Some(marker)).expect("state");
}

fn stage_of(store: &StateStore) -> Option<PendingStage> {
    store.load().pending.map(|p| p.stage)
}

#[test]
fn a_release_that_never_reached_a_working_session_is_rolled_back_at_the_next_login() {
    let (sandbox, store) = two_releases();
    mark(&store, PendingStage::AwaitingFirstLogin);
    let helper = Recording {
        sandbox: &sandbox,
        calls: Mutex::new(Vec::new()),
    };

    // First start after the install, in the next login: the first login. Nothing is undone yet.
    let first = on_start(&sandbox.layout, &store, &helper, &login("2"));
    assert_eq!(first, Health::Fine);
    assert_eq!(stage_of(&store), Some(PendingStage::FirstLoginSeen));
    assert_eq!(
        store.load().pending.and_then(|p| p.session),
        Some(login("2")),
        "the marker records which login it was"
    );
    assert!(helper.calls.lock().expect("lock").is_empty());

    // The next login, still unconfirmed: the first login never made it.
    let second = on_start(&sandbox.layout, &store, &helper, &login("3"));
    assert_eq!(second, Health::Restored);
    assert_eq!(*helper.calls.lock().expect("lock"), vec!["rollback"]);
    assert_eq!(sandbox.current(), Some(v("0.1.0")));
    assert_eq!(store.load().pending, None);
    assert_eq!(
        store.load().refused,
        Some(v("0.2.0")),
        "a release that was rolled back is remembered as refused"
    );
}

#[test]
fn a_daemon_restart_inside_a_session_is_not_a_failed_login() {
    let (sandbox, store) = two_releases();
    mark(&store, PendingStage::AwaitingFirstLogin);
    let helper = Recording {
        sandbox: &sandbox,
        calls: Mutex::new(Vec::new()),
    };

    // Restarted in the session that installed it: still waiting for the first login.
    let installing = on_start(&sandbox.layout, &store, &helper, &login("1"));
    assert_eq!(installing, Health::Fine);
    assert_eq!(stage_of(&store), Some(PendingStage::AwaitingFirstLogin));

    // The first login starts; the daemon then crashes and restarts twice inside it.
    for _ in 0..3 {
        let health = on_start(&sandbox.layout, &store, &helper, &login("2"));
        assert_eq!(health, Health::Fine);
        assert_eq!(stage_of(&store), Some(PendingStage::FirstLoginSeen));
    }
    assert!(helper.calls.lock().expect("lock").is_empty());
    assert_eq!(sandbox.current(), Some(v("0.2.0")));

    // Same session number after a reboot is another login.
    let rebooted = LoginSession {
        boot: Some("b2".to_owned()),
        ..login("2")
    };
    let after = on_start(&sandbox.layout, &store, &helper, &rebooted);
    assert_eq!(after, Health::Restored);
    assert_eq!(sandbox.current(), Some(v("0.1.0")));
}

#[test]
fn a_confirmed_release_is_left_alone() {
    let (sandbox, store) = two_releases();
    mark(&store, PendingStage::FirstLoginSeen);
    assert!(slipway::cli::confirm(&sandbox.layout.root, &store).expect("confirm"));
    let helper = Recording {
        sandbox: &sandbox,
        calls: Mutex::new(Vec::new()),
    };
    let health = on_start(&sandbox.layout, &store, &helper, &login("3"));
    assert_eq!(health, Health::Fine);
    assert!(helper.calls.lock().expect("lock").is_empty());
    assert_eq!(sandbox.current(), Some(v("0.2.0")));
    assert_eq!(store.load().pending, None);
    assert_eq!(store.load().confirmed, Some(v("0.2.0")));
}

#[test]
fn nothing_pending_means_nothing_happens() {
    let (sandbox, store) = two_releases();
    let helper = Recording {
        sandbox: &sandbox,
        calls: Mutex::new(Vec::new()),
    };
    let health = on_start(&sandbox.layout, &store, &helper, &login("1"));
    assert_eq!(health, Health::Fine);
    assert!(helper.calls.lock().expect("lock").is_empty());
}

#[test]
fn a_failed_rollback_is_not_reported_as_restored() {
    struct Broken;
    impl Elevate for Broken {
        fn install(&self, _: &Path) -> Result<Report, ApplyFailure> {
            Ok(Report::default())
        }
        fn rollback(&self) -> Result<Report, ApplyFailure> {
            Err(ApplyFailure {
                code: Some(1),
                detail: "disk".into(),
            })
        }
    }
    let (sandbox, store) = two_releases();
    mark(&store, PendingStage::FirstLoginSeen);
    let health = on_start(&sandbox.layout, &store, &Broken, &login("3"));
    assert_eq!(health, Health::Fine);
    assert!(
        store.load().pending.is_some(),
        "the marker stays so the next start tries again"
    );
    assert_eq!(store.load().refused, None, "nothing was rolled back");
}
