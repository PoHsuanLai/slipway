//! The session's two calls: `confirm` once the shell is up and `recover` before the compositor starts.
//! The root side runs against a temporary install root; the helper's answers are faked.

use crate::support::{Accept, Recipe, Sandbox, build, v};
use slipway::apply::{NoRelabel, Started, note_confirmed, note_start};
use slipway::cli::{Exit, confirm, recover, vouch};
use slipway::elevate::{ApplyFailure, Elevate, Recovery};
use slipway::report::Report;
use slipway::state::StateStore;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Mutex;

fn two_releases() -> Sandbox {
    let sandbox = Sandbox::new();
    for version in ["0.1.0", "0.2.0"] {
        sandbox
            .install(&build(&Recipe::new(version)))
            .expect("install");
    }
    sandbox
}

fn start(sandbox: &Sandbox, boot: &str) -> Started {
    note_start(&sandbox.layout, &Accept, &NoRelabel, boot).expect("start")
}

fn is_fine(started: &Started) -> bool {
    matches!(started, Started::Fine)
}

#[test]
fn a_release_that_never_starts_is_rolled_back_on_the_third_different_boot() {
    let sandbox = two_releases();
    assert!(is_fine(&start(&sandbox, "b1")));
    assert!(is_fine(&start(&sandbox, "b2")));
    assert_eq!(sandbox.current(), Some(v("0.2.0")));
    match start(&sandbox, "b3") {
        Started::Restored(applied) => assert_eq!(applied.version, v("0.1.0")),
        Started::Fine => panic!("the third unconfirmed boot restores the previous version"),
    }
    assert_eq!(sandbox.current(), Some(v("0.1.0")));

    // The release that was put back is known to work: it is not rolled back for its neighbour's boots.
    for boot in ["b4", "b5", "b6", "b7"] {
        assert!(is_fine(&start(&sandbox, boot)), "{boot}");
    }
    assert_eq!(sandbox.current(), Some(v("0.1.0")));
}

#[test]
fn the_same_boot_counts_once_so_a_program_of_the_person_cannot_force_a_rollback() {
    let sandbox = two_releases();
    for _ in 0..50 {
        assert!(is_fine(&start(&sandbox, "b1")));
    }
    assert_eq!(sandbox.current(), Some(v("0.2.0")));
}

#[test]
fn a_confirmed_release_keeps_its_boots() {
    let sandbox = two_releases();
    assert!(is_fine(&start(&sandbox, "b1")));
    assert!(is_fine(&start(&sandbox, "b2")));
    note_confirmed(&sandbox.layout).expect("confirmed");
    for boot in ["b3", "b4", "b5", "b6"] {
        assert!(is_fine(&start(&sandbox, boot)), "{boot}");
    }
    assert_eq!(sandbox.current(), Some(v("0.2.0")));
    // Confirming twice is the same as once.
    note_confirmed(&sandbox.layout).expect("confirmed again");
    assert!(is_fine(&start(&sandbox, "b7")));
}

#[test]
fn a_new_release_starts_its_own_count() {
    let sandbox = two_releases();
    assert!(is_fine(&start(&sandbox, "b1")));
    assert!(is_fine(&start(&sandbox, "b2")));
    sandbox
        .install(&build(&Recipe::new("0.3.0")))
        .expect("install");
    assert!(is_fine(&start(&sandbox, "b3")), "0.3.0 has had one boot");
    assert!(is_fine(&start(&sandbox, "b4")));
    assert_eq!(sandbox.current(), Some(v("0.3.0")));
}

#[test]
fn with_nothing_to_go_back_to_or_nothing_installed_nothing_happens() {
    let only = Sandbox::new();
    only.install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    for boot in ["b1", "b2", "b3", "b4", "b5"] {
        assert!(is_fine(&start(&only, boot)), "{boot}");
    }
    assert_eq!(only.current(), Some(v("0.1.0")));

    let none = Sandbox::new();
    assert!(is_fine(&start(&none, "b1")));
    note_confirmed(&none.layout).expect("nothing to confirm");
    assert!(
        !none.layout.root.exists(),
        "a computer without an install is not given one"
    );
}

#[test]
fn the_record_is_for_root_alone() {
    let sandbox = two_releases();
    assert!(is_fine(&start(&sandbox, "b1")));
    let mode = std::fs::metadata(sandbox.layout.health_file())
        .expect("record")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    assert_eq!(
        sandbox.layout.health_file().parent(),
        Some(sandbox.layout.private_dir().as_path())
    );
}

/// Records what the session asks of the helper, and answers as set.
struct Helper {
    calls: Mutex<Vec<&'static str>>,
    recover: Result<Recovery, ApplyFailure>,
    confirmed: Result<(), ApplyFailure>,
}

impl Helper {
    fn answering(recover: Result<Recovery, ApplyFailure>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            recover,
            confirmed: Ok(()),
        }
    }
}

impl Elevate for Helper {
    fn install(&self, _: &Path) -> Result<Report, ApplyFailure> {
        Ok(Report::default())
    }

    fn rollback(&self) -> Result<Report, ApplyFailure> {
        Ok(Report::default())
    }

    fn recover(&self) -> Result<Recovery, ApplyFailure> {
        self.calls.lock().expect("lock").push("recover");
        self.recover.clone()
    }

    fn confirmed(&self) -> Result<(), ApplyFailure> {
        self.calls.lock().expect("lock").push("confirmed");
        self.confirmed.clone()
    }
}

fn failure() -> ApplyFailure {
    ApplyFailure {
        code: Some(1),
        detail: "no".into(),
    }
}

#[test]
fn confirm_records_in_both_places_and_is_harmless_twice() {
    let sandbox = two_releases();
    let store = StateStore::new(sandbox.tmp.path().join("state/state.toml"));
    let helper = Helper::answering(Ok(Recovery::Fine));
    for _ in 0..2 {
        let reply = vouch(&sandbox.layout.root, &store, &helper);
        assert_eq!(reply.exit, Exit::Done, "{}", reply.text);
        assert_eq!(store.load().confirmed, Some(v("0.2.0")));
    }
    assert_eq!(*helper.calls.lock().expect("lock"), vec!["confirmed"; 2]);

    // Confirming what is already confirmed writes nothing: it works even where the state folder can
    // no longer be written to.
    let folder = sandbox.tmp.path().join("state");
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).expect("lock");
    let again = confirm(&sandbox.layout.root, &store);
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).expect("unlock");
    assert!(again.expect("nothing to write"));
}

#[test]
fn confirm_says_so_when_the_system_could_not_be_told() {
    let sandbox = two_releases();
    let store = StateStore::new(sandbox.tmp.path().join("state.toml"));
    let helper = Helper {
        confirmed: Err(failure()),
        ..Helper::answering(Ok(Recovery::Fine))
    };
    let reply = vouch(&sandbox.layout.root, &store, &helper);
    assert_eq!(reply.exit, Exit::Failed);
    assert!(
        !reply.text.to_lowercase().contains("polkit") && !reply.text.contains("helper"),
        "{}",
        reply.text
    );
}

#[test]
fn confirm_on_a_computer_without_an_install_asks_for_nothing() {
    let sandbox = Sandbox::new();
    let store = StateStore::new(sandbox.tmp.path().join("state.toml"));
    let helper = Helper::answering(Ok(Recovery::Fine));
    let reply = vouch(&sandbox.layout.root, &store, &helper);
    assert_eq!(reply.exit, Exit::Done);
    assert!(helper.calls.lock().expect("lock").is_empty());
}

#[test]
fn recover_tells_the_session_what_happened() {
    let table = [
        (Ok(Recovery::Fine), Exit::Done),
        (Ok(Recovery::Restored(Report::default())), Exit::Restored),
        (Err(failure()), Exit::Failed),
    ];
    for (answer, expected) in table {
        let reply = recover(&Helper::answering(answer));
        assert_eq!(reply.exit, expected, "{}", reply.text);
    }
    assert_eq!(Exit::Restored as u8, 5);
}
