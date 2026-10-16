//! The helper is started by a process of the person, so everything that process can influence is
//! hostile: the umask, the files it names, the files it keeps open. None of these tests needs root.

use crate::support::{Accept, Recipe, Reject, Sandbox, build, read, v};
use rustix::fs::{Mode, mkfifoat};
use slipway_root::apply::harden::set_umask;
use slipway_root::apply::open::open_regular;
use slipway_root::apply::{ApplyError, Back, Staged, StagedError, install, rollback};
use std::io::Read;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

fn mode_of(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777
}

/// Every file and folder under `dir`, with its mode.
fn walk(dir: &Path, out: &mut Vec<(std::path::PathBuf, bool, u32)>) {
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("entry").path();
        let meta = std::fs::symlink_metadata(&path).expect("metadata");
        out.push((
            path.clone(),
            meta.is_dir(),
            meta.permissions().mode() & 0o777,
        ));
        if meta.is_dir() {
            walk(&path, out);
        }
    }
}

#[test]
fn nothing_the_helper_makes_is_writable_by_anyone_else_whatever_the_umask() {
    // The helper sets its own umask first thing.
    set_umask();
    let was = rustix::process::umask(Mode::empty());
    assert_eq!(was.bits(), 0o022);

    for mask in [0o000, 0o077] {
        let sandbox = Sandbox::new();
        let _ = rustix::process::umask(Mode::from_bits_truncate(mask));
        let first = sandbox.install(&build(&Recipe::new("0.1.0")));
        let second = sandbox.install(&build(&Recipe::new("0.2.0")));
        set_umask();
        first.expect("install under this umask");
        second.expect("upgrade under this umask");

        let mut all = Vec::new();
        walk(&sandbox.layout.root, &mut all);
        assert!(!all.is_empty());
        for (path, is_dir, mode) in &all {
            if std::fs::symlink_metadata(path)
                .expect("metadata")
                .file_type()
                .is_symlink()
            {
                continue;
            }
            assert_eq!(
                mode & 0o022,
                0,
                "umask {mask:o}: {} is {mode:o}",
                path.display()
            );
            if *is_dir && !path.ends_with(".private") {
                assert_eq!(*mode, 0o755, "umask {mask:o}: {}", path.display());
            }
        }
        let record = sandbox.layout.record(&v("0.2.0"));
        assert_eq!(mode_of(&record), 0o644, "umask {mask:o}");
        assert_eq!(
            mode_of(record.parent().expect("record folder")),
            0o755,
            "umask {mask:o}"
        );
        assert_eq!(
            mode_of(&sandbox.layout.private_dir()),
            0o700,
            "umask {mask:o}"
        );
    }
}

#[test]
fn the_lock_is_where_only_root_can_open_it() {
    let sandbox = Sandbox::new();
    sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    let lock = sandbox.layout.lock_file();
    assert_eq!(lock.parent(), Some(sandbox.layout.private_dir().as_path()));
    assert_eq!(mode_of(&sandbox.layout.private_dir()), 0o700);
    assert_eq!(mode_of(&lock), 0o600);
    assert!(
        !sandbox.layout.root.join("lock").exists(),
        "no lock file in a folder everybody can enter"
    );
}

#[test]
fn a_private_folder_of_someone_else_or_a_link_in_its_place_is_refused() {
    let sandbox = Sandbox::new();
    sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    let private = sandbox.layout.private_dir();
    let elsewhere = sandbox.tmp.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).expect("dir");
    std::fs::remove_dir_all(&private).expect("remove");
    symlink(&elsewhere, &private).expect("symlink");
    let err = sandbox
        .install(&build(&Recipe::new("0.2.0")))
        .expect_err("a link is not our folder");
    assert!(matches!(err, ApplyError::Io { .. }), "{err}");
    assert!(
        std::fs::read_dir(&elsewhere).expect("dir").next().is_none(),
        "nothing was written through the link"
    );
}

/// Runs `work` on another thread and fails the test if it has not finished in time: the bug being
/// tested is a helper that waits for ever.
fn within<T: Send + 'static>(what: &str, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx.recv_timeout(Duration::from_secs(20))
        .unwrap_or_else(|_| panic!("{what}: still waiting after 20 seconds"))
}

#[test]
fn a_staged_file_that_is_a_pipe_or_a_link_is_refused_without_waiting() {
    let table = ["release.toml", "release.toml.minisig", "bundle.tar.zst"];
    for name in table {
        for kind in ["pipe", "link"] {
            let sandbox = Sandbox::new();
            let dir = sandbox.stage(&build(&Recipe::new("0.1.0")));
            let target = dir.join(name);
            let real = sandbox.tmp.path().join("real");
            std::fs::rename(&target, &real).expect("move away");
            match kind {
                "pipe" => mkfifoat(rustix::fs::CWD, target.as_path(), Mode::RUSR | Mode::WUSR)
                    .expect("fifo"),
                _ => symlink(&real, &target).expect("symlink"),
            }
            let layout = sandbox.layout.clone();
            let err = within(&format!("{kind} as {name}"), move || {
                install(&layout, &dir, &Accept)
            })
            .expect_err("refused");
            assert!(
                matches!(err, ApplyError::Staged(StagedError::Io { .. })),
                "{kind} as {name}: {err}"
            );
            assert_eq!(sandbox.current(), None);
        }
    }
}

#[test]
fn only_a_regular_file_is_opened() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("file");
    std::fs::write(&file, "text").expect("file");
    let mut text = String::new();
    open_regular(&file)
        .expect("a regular file opens")
        .read_to_string(&mut text)
        .expect("read");
    assert_eq!(text, "text");

    let pipe = dir.path().join("pipe");
    mkfifoat(rustix::fs::CWD, pipe.as_path(), Mode::RUSR | Mode::WUSR).expect("fifo");
    let link = dir.path().join("link");
    symlink(&file, &link).expect("symlink");
    let missing = dir.path().join("missing");
    let refused: [&Path; 4] = [&pipe, &link, dir.path(), &missing];
    for path in refused {
        let path = path.to_owned();
        within("opening", move || open_regular(&path).expect_err("refused"));
    }
}

#[test]
fn the_unpacked_bytes_are_the_ones_that_were_hashed_even_if_the_file_changes_afterwards() {
    let sandbox = Sandbox::new();
    let built = build(&Recipe::new("0.1.0"));
    let dir = sandbox.stage(&built);
    let vault = tempfile::tempdir().expect("vault");

    let sealed = Staged::describe(&dir, &Accept)
        .expect("describe")
        .seal(vault.path())
        .expect("seal");
    // The person's process rewrites the file it named, through its own handle or by path.
    std::fs::write(dir.join("bundle.tar.zst"), b"something else entirely").expect("swap");

    let mut copy = Vec::new();
    sealed
        .bundle()
        .read_to_end(&mut copy)
        .expect("read the private copy");
    assert_eq!(copy, built.bundle, "the copy is what the digest covered");
    assert_eq!(
        std::fs::read_dir(vault.path()).expect("vault").count(),
        0,
        "the copy has no name for anyone to open"
    );
}

#[test]
fn an_oversize_stream_is_cut_at_the_listed_size() {
    let sandbox = Sandbox::new();
    let built = build(&Recipe::new("0.1.0"));
    let dir = sandbox.stage(&built);
    let mut huge = built.bundle.clone();
    huge.extend(std::iter::repeat_n(0u8, 5_000_000));
    std::fs::write(dir.join("bundle.tar.zst"), &huge).expect("grow");
    let vault = tempfile::tempdir().expect("vault");
    let err = Staged::describe(&dir, &Accept)
        .expect("describe")
        .seal(vault.path())
        .expect_err("too long");
    assert!(matches!(err, StagedError::BundleSize { .. }), "{err}");
    assert!(
        !err.to_string().contains("5000000"),
        "the error does not say how big the named file was: {err}"
    );
}

#[test]
fn a_system_file_changed_after_it_was_unpacked_is_not_placed_and_the_old_release_comes_back() {
    let sandbox = Sandbox::new();
    let udev = "/etc/udev/rules.d/70-keycap.rules";
    let in_bundle = "system/etc/udev/rules.d/70-keycap.rules";
    for (version, rule) in [("0.1.0", "v1"), ("0.2.0", "v2")] {
        let mut recipe = Recipe::new(version);
        recipe.listed.push((in_bundle, rule));
        recipe.system.push(udev);
        sandbox.install(&build(&recipe)).expect("install");
    }
    assert_eq!(read(&sandbox.link(udev)), "v2");

    // Somebody swaps the file in the release being returned to.
    let kept = sandbox.layout.release_dir(&v("0.1.0")).join(in_bundle);
    std::fs::write(&kept, "polkit.addRule(() => polkit.Result.YES)").expect("tamper");

    let err = rollback(&sandbox.layout, &Accept).expect_err("not what was signed");
    match &err {
        ApplyError::AfterSwitch { cause, back } => {
            assert!(matches!(**cause, ApplyError::Changed(_)), "{err}");
            assert_eq!(*back, Back::Restored);
        }
        other => panic!("wrong refusal: {other}"),
    }
    assert_eq!(err.exit_code(), slipway_root::apply::ExitCode::Unverified);
    assert_eq!(read(&sandbox.link(udev)), "v2", "nothing was placed");
    assert_eq!(
        sandbox.current(),
        Some(v("0.2.0")),
        "the earlier release is back"
    );
    assert_eq!(sandbox.previous(), Some(v("0.1.0")));
}

#[test]
fn an_installed_record_is_trusted_only_as_far_as_the_key_vouches_for_it() {
    let sandbox = Sandbox::new();
    for version in ["0.1.0", "0.2.0"] {
        sandbox
            .install(&build(&Recipe::new(version)))
            .expect("install");
    }
    let err = rollback(&sandbox.layout, &Reject).expect_err("the record does not verify");
    assert!(
        matches!(err, ApplyError::Staged(StagedError::Signature(_))),
        "{err}"
    );
    assert_eq!(sandbox.current(), Some(v("0.2.0")), "nothing moved");

    // A record edited after the install is not a release any more (with a real key its signature
    // would not match either); nothing moves for it.
    let record = sandbox.layout.record(&v("0.1.0"));
    std::fs::write(&record, "format = 1\n").expect("edit the record");
    rollback(&sandbox.layout, &Accept).expect_err("an edited record does not parse as a release");
    assert_eq!(sandbox.current(), Some(v("0.2.0")));
}
