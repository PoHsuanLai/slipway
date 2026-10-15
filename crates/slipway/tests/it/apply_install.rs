use crate::support::{Recipe, Sandbox, build, read, v};
use std::os::unix::fs::PermissionsExt;

#[test]
fn a_first_install_unpacks_switches_and_links() {
    let sandbox = Sandbox::new();
    let applied = sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    assert_eq!(applied.version, v("0.1.0"));
    assert!(applied.conflicts.is_empty());
    assert_eq!(sandbox.current(), Some(v("0.1.0")));
    assert_eq!(sandbox.previous(), None);
    let current = sandbox.layout.current();
    assert_eq!(read(&current.join("bin/casement")), "casement 1");
    assert_eq!(
        std::fs::metadata(current.join("bin/casement"))
            .expect("meta")
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        std::fs::metadata(current.join("share/wayland-sessions/quire.desktop"))
            .expect("meta")
            .permissions()
            .mode()
            & 0o777,
        0o644
    );
    // Every folder is open to all readers, whatever umask the helper was started with.
    for dir in [
        sandbox.layout.root.clone(),
        sandbox.layout.releases_dir(),
        sandbox.layout.release_dir(&v("0.1.0")),
        sandbox
            .layout
            .release_dir(&v("0.1.0"))
            .join("share/wayland-sessions"),
    ] {
        let mode = std::fs::metadata(&dir).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o755, "{}", dir.display());
    }
    assert!(sandbox.layout.record(&v("0.1.0")).is_file());
    // The export reads through the link into the current release.
    let link = sandbox.link("/usr/share/wayland-sessions/quire.desktop");
    assert_eq!(read(&link), "[Desktop Entry]");
    assert_eq!(
        std::fs::read_link(&link).expect("link"),
        sandbox
            .layout
            .root
            .join("current/share/wayland-sessions/quire.desktop")
    );
    assert_eq!(sandbox.releases(), vec!["0.1.0"]);
}

#[test]
fn an_upgrade_keeps_the_old_release_as_previous_and_prunes_older_ones() {
    let sandbox = Sandbox::new();
    for version in ["0.1.0", "0.2.0"] {
        sandbox
            .install(&build(&Recipe::new(version)))
            .expect("install");
    }
    assert_eq!(sandbox.current(), Some(v("0.2.0")));
    assert_eq!(sandbox.previous(), Some(v("0.1.0")));
    assert_eq!(sandbox.releases(), vec!["0.1.0", "0.2.0"]);
    assert_eq!(
        std::fs::read_link(sandbox.layout.current()).expect("link"),
        std::path::PathBuf::from("releases/0.2.0")
    );

    sandbox
        .install(&build(&Recipe::new("0.3.0")))
        .expect("install");
    assert_eq!(sandbox.current(), Some(v("0.3.0")));
    assert_eq!(sandbox.previous(), Some(v("0.2.0")));
    assert_eq!(sandbox.releases(), vec!["0.2.0", "0.3.0"]);
}

#[test]
fn a_release_that_is_not_newer_is_refused_and_changes_nothing() {
    let sandbox = Sandbox::new();
    sandbox
        .install(&build(&Recipe::new("0.2.0")))
        .expect("install");
    for version in ["0.2.0", "0.1.0"] {
        let err = sandbox
            .install(&build(&Recipe::new(version)))
            .expect_err(version);
        assert!(
            matches!(err, slipway::apply::ApplyError::Plan(_)),
            "{version}: {err}"
        );
        assert_eq!(err.exit_code(), slipway::apply::ExitCode::Refused);
    }
    assert_eq!(sandbox.current(), Some(v("0.2.0")));
    assert_eq!(sandbox.releases(), vec!["0.2.0"]);
}

#[test]
fn leftovers_of_an_earlier_attempt_do_not_get_in_the_way() {
    let sandbox = Sandbox::new();
    let partial = sandbox.layout.partial_dir(&v("0.1.0"));
    std::fs::create_dir_all(partial.join("junk")).expect("partial");
    std::fs::write(partial.join("junk/file"), "old").expect("junk");
    let unswitched = sandbox.layout.release_dir(&v("0.1.0"));
    std::fs::create_dir_all(&unswitched).expect("release dir");
    std::fs::write(unswitched.join("stale"), "old").expect("stale");

    sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    assert!(
        !sandbox
            .layout
            .release_dir(&v("0.1.0"))
            .join("stale")
            .exists()
    );
    assert!(!partial.exists());
}

#[test]
fn a_second_update_at_the_same_time_is_turned_away() {
    let sandbox = Sandbox::new();
    std::fs::create_dir_all(sandbox.layout.releases_dir()).expect("dirs");
    let held = std::fs::File::create(sandbox.layout.lock_file()).expect("lock file");
    held.lock().expect("lock");
    let err = sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect_err("busy");
    assert_eq!(err.exit_code(), slipway::apply::ExitCode::Busy);
    drop(held);
    sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("free again");
}
