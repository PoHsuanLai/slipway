use crate::support::{Recipe, Sandbox, build, v};
use slipway::elevate::{ApplyFailure, Elevate};
use slipway::health::{Health, on_start};
use slipway::state::StateStore;
use slipway_core::{Pending, PendingStage};
use std::path::Path;
use std::sync::Mutex;

/// Stands in for the privileged helper: records the calls and rolls back in the sandbox.
struct Recording<'a> {
    sandbox: &'a Sandbox,
    calls: Mutex<Vec<&'static str>>,
}

impl Elevate for Recording<'_> {
    fn install(&self, _: &Path) -> Result<(), ApplyFailure> {
        self.calls.lock().expect("lock").push("install");
        Ok(())
    }

    fn rollback(&self) -> Result<(), ApplyFailure> {
        self.calls.lock().expect("lock").push("rollback");
        slipway::apply::rollback(&self.sandbox.layout)
            .map(drop)
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

fn mark(store: &StateStore, stage: PendingStage) {
    store
        .update(|s| {
            s.pending = Some(Pending {
                version: v("0.2.0"),
                stage,
            })
        })
        .expect("state");
}

#[test]
fn a_release_that_never_reached_a_working_session_is_rolled_back_on_the_second_start() {
    let (sandbox, store) = two_releases();
    mark(&store, PendingStage::AwaitingFirstLogin);
    let helper = Recording {
        sandbox: &sandbox,
        calls: Mutex::new(Vec::new()),
    };

    // First start after the install: the first login. Nothing is undone yet.
    assert_eq!(on_start(&sandbox.layout, &store, &helper), Health::Fine);
    assert_eq!(
        store.load().pending.map(|p| p.stage),
        Some(PendingStage::FirstLoginSeen)
    );
    assert!(helper.calls.lock().expect("lock").is_empty());

    // Second start, still unconfirmed: the first login never made it.
    assert_eq!(on_start(&sandbox.layout, &store, &helper), Health::Restored);
    assert_eq!(*helper.calls.lock().expect("lock"), vec!["rollback"]);
    assert_eq!(sandbox.current(), Some(v("0.1.0")));
    assert_eq!(store.load().pending, None);
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
    assert_eq!(on_start(&sandbox.layout, &store, &helper), Health::Fine);
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
    assert_eq!(on_start(&sandbox.layout, &store, &helper), Health::Fine);
    assert!(helper.calls.lock().expect("lock").is_empty());
}

#[test]
fn a_failed_rollback_is_not_reported_as_restored() {
    struct Broken;
    impl Elevate for Broken {
        fn install(&self, _: &Path) -> Result<(), ApplyFailure> {
            Ok(())
        }
        fn rollback(&self) -> Result<(), ApplyFailure> {
            Err(ApplyFailure {
                code: Some(1),
                detail: "disk".into(),
            })
        }
    }
    let (sandbox, store) = two_releases();
    mark(&store, PendingStage::FirstLoginSeen);
    assert_eq!(on_start(&sandbox.layout, &store, &Broken), Health::Fine);
    assert!(
        store.load().pending.is_some(),
        "the marker stays so the next start tries again"
    );
}
