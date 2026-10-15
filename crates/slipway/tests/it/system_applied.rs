use crate::support::{Accept, Recipe, Sandbox, build, read, v};
use slipway::apply::{Ran, Reach, Relabel, Relabeled, RunProgram, Selinux, install_with, rollback};
use std::cell::RefCell;
use std::ffi::OsString;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

const UDEV: &str = "/etc/udev/rules.d/70-keycap.rules";
const UDEV_IN_BUNDLE: &str = "system/etc/udev/rules.d/70-keycap.rules";
const PAM: &str = "/etc/pam.d/sill";
const PAM_IN_BUNDLE: &str = "system/etc/pam.d/sill";

/// A release placing the udev rule with these contents.
fn placing<'a>(version: &'a str, udev: &'a str) -> Recipe<'a> {
    let mut recipe = Recipe::new(version);
    recipe.listed.push((UDEV_IN_BUNDLE, udev));
    recipe.system.push(UDEV);
    recipe
}

#[test]
fn a_system_file_is_placed_replaced_and_dropped_with_the_releases() {
    let sandbox = Sandbox::new();
    let at = sandbox.link(UDEV);

    let applied = sandbox
        .install(&build(&placing("0.1.0", "v1")))
        .expect("install");
    assert!(applied.conflicts.is_empty());
    assert_eq!(read(&at), "v1");
    assert!(
        !std::fs::symlink_metadata(&at)
            .expect("meta")
            .file_type()
            .is_symlink(),
        "a system file is a real file, not a link into the root"
    );
    assert_eq!(
        std::fs::metadata(&at).expect("meta").permissions().mode() & 0o777,
        0o644
    );

    sandbox
        .install(&build(&placing("0.2.0", "v2")))
        .expect("install");
    assert_eq!(read(&at), "v2");

    sandbox
        .install(&build(&Recipe::new("0.3.0")))
        .expect("install");
    assert!(
        !at.exists(),
        "a file the new release no longer lists is removed"
    );
}

#[test]
fn a_file_the_person_owns_is_never_replaced_or_removed() {
    let sandbox = Sandbox::new();
    let at = sandbox.link(UDEV);
    std::fs::create_dir_all(at.parent().expect("parent")).expect("dirs");
    std::fs::write(&at, "my own rule").expect("file");

    let applied = sandbox
        .install(&build(&placing("0.1.0", "v1")))
        .expect("install");
    assert_eq!(applied.conflicts, vec![UDEV.to_owned()]);
    assert_eq!(read(&at), "my own rule");

    // Not removed when a later release drops it either.
    sandbox
        .install(&build(&Recipe::new("0.2.0")))
        .expect("install");
    assert_eq!(read(&at), "my own rule");
}

#[test]
fn a_file_of_ours_that_was_edited_is_left_alone() {
    let sandbox = Sandbox::new();
    let at = sandbox.link(UDEV);
    sandbox
        .install(&build(&placing("0.1.0", "v1")))
        .expect("install");
    std::fs::write(&at, "v1 edited").expect("edit");

    let applied = sandbox
        .install(&build(&placing("0.2.0", "v2")))
        .expect("install");
    assert_eq!(applied.conflicts, vec![UDEV.to_owned()]);
    assert_eq!(read(&at), "v1 edited");
}

#[test]
fn a_link_in_the_way_is_not_followed() {
    let sandbox = Sandbox::new();
    let at = sandbox.link(UDEV);
    let elsewhere = sandbox.tmp.path().join("elsewhere");
    std::fs::write(&elsewhere, "untouched").expect("file");
    std::fs::create_dir_all(at.parent().expect("parent")).expect("dirs");
    symlink(&elsewhere, &at).expect("symlink");

    let applied = sandbox
        .install(&build(&placing("0.1.0", "v1")))
        .expect("install");
    assert_eq!(applied.conflicts, vec![UDEV.to_owned()]);
    assert_eq!(read(&elsewhere), "untouched");
}

#[test]
fn a_rollback_puts_the_earlier_system_files_back() {
    let sandbox = Sandbox::new();
    let at = sandbox.link(UDEV);
    let pam = sandbox.link(PAM);
    sandbox
        .install(&build(&placing("0.1.0", "v1")))
        .expect("install");
    let mut second = placing("0.2.0", "v2");
    second.listed.push((PAM_IN_BUNDLE, "pam"));
    second.system.push(PAM);
    sandbox.install(&build(&second)).expect("install");
    assert_eq!(read(&at), "v2");
    assert_eq!(read(&pam), "pam");

    let back = rollback(&sandbox.layout).expect("rollback");
    assert_eq!(back.version, v("0.1.0"));
    assert_eq!(read(&at), "v1");
    assert!(
        !pam.exists(),
        "a file only the release we left had is removed"
    );
}

#[test]
fn a_release_listing_a_system_file_outside_the_allowed_folders_is_refused_whole() {
    let sandbox = Sandbox::new();
    let mut recipe = Recipe::new("0.1.0");
    recipe.listed.push(("system/etc/shadow", "root::0:0:::::"));
    recipe.system.push("/etc/shadow");
    sandbox.install(&build(&recipe)).expect_err("refused");
    assert_eq!(sandbox.current(), None);
    assert!(!sandbox.link("/etc/shadow").exists());
}

/// Records what it was asked to relabel.
#[derive(Default)]
struct Recorder(RefCell<Vec<(Vec<PathBuf>, Reach)>>);

impl Relabel for Recorder {
    fn relabel(&self, paths: &[PathBuf], reach: Reach) -> Relabeled {
        self.0.borrow_mut().push((paths.to_vec(), reach));
        Relabeled::Done
    }
}

#[test]
fn an_install_relabels_the_root_then_the_system_files() {
    let sandbox = Sandbox::new();
    let recorder = Recorder::default();
    let built = build(&placing("0.1.0", "v1"));
    install_with(&sandbox.layout, &sandbox.stage(&built), &Accept, &recorder).expect("install");
    assert_eq!(
        *recorder.0.borrow(),
        vec![
            (vec![sandbox.layout.root.clone()], Reach::Tree),
            (vec![sandbox.link(UDEV)], Reach::Paths),
        ]
    );
}

/// Answers for a program by path, and records the calls.
struct Fake {
    answers: Vec<(&'static str, Ran)>,
    calls: RefCell<Vec<(PathBuf, Vec<OsString>)>>,
}

impl Fake {
    fn new(answers: Vec<(&'static str, Ran)>) -> Self {
        Self {
            answers,
            calls: RefCell::default(),
        }
    }
}

impl RunProgram for &Fake {
    fn run(&self, program: &Path, args: &[OsString]) -> Ran {
        self.calls
            .borrow_mut()
            .push((program.to_owned(), args.to_vec()));
        self.answers
            .iter()
            .find(|(p, _)| Path::new(p) == program)
            .map_or(Ran::Missing, |(_, ran)| ran.clone())
    }
}

#[test]
fn relabelling_runs_restorecon_only_where_selinux_is_in_use_and_never_fails_an_install() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let on = tmp.path().join("enforce");
    std::fs::write(&on, "1").expect("status");
    let off = tmp.path().join("no-selinux");
    let tools = || {
        vec![
            PathBuf::from("/a/restorecon"),
            PathBuf::from("/b/restorecon"),
        ]
    };
    let root = vec![PathBuf::from("/opt/quire")];

    // (case, status file, what the tools answer, paths, reach, outcome, calls made)
    type Case = (
        &'static str,
        PathBuf,
        Vec<(&'static str, Ran)>,
        Vec<PathBuf>,
        Reach,
        Relabeled,
        usize,
    );
    let table: Vec<Case> = vec![
        (
            "SELinux not in use",
            off.clone(),
            vec![("/a/restorecon", Ran::Succeeded)],
            root.clone(),
            Reach::Tree,
            Relabeled::NotNeeded,
            0,
        ),
        (
            "nothing to relabel",
            on.clone(),
            vec![("/a/restorecon", Ran::Succeeded)],
            vec![],
            Reach::Paths,
            Relabeled::NotNeeded,
            0,
        ),
        (
            "tool missing everywhere",
            on.clone(),
            vec![],
            root.clone(),
            Reach::Tree,
            Relabeled::NotNeeded,
            2,
        ),
        (
            "first tool missing, second works",
            on.clone(),
            vec![("/b/restorecon", Ran::Succeeded)],
            root.clone(),
            Reach::Tree,
            Relabeled::Done,
            2,
        ),
        (
            "first tool works",
            on.clone(),
            vec![("/a/restorecon", Ran::Succeeded)],
            root.clone(),
            Reach::Paths,
            Relabeled::Done,
            1,
        ),
        (
            "tool fails",
            on.clone(),
            vec![("/a/restorecon", Ran::Failed("denied".into()))],
            root.clone(),
            Reach::Tree,
            Relabeled::Failed("denied".into()),
            1,
        ),
    ];
    for (name, status, answers, paths, reach, want, calls) in table {
        let fake = Fake::new(answers);
        let selinux = Selinux::new(status, tools(), &fake);
        assert_eq!(selinux.relabel(&paths, reach), want, "{name}");
        assert_eq!(fake.calls.borrow().len(), calls, "{name}");
    }

    let fake = Fake::new(vec![("/a/restorecon", Ran::Succeeded)]);
    Selinux::new(on.clone(), tools(), &fake).relabel(&root, Reach::Tree);
    Selinux::new(on, tools(), &fake).relabel(&root, Reach::Paths);
    let calls = fake.calls.borrow();
    assert_eq!(
        calls[0].1,
        vec![OsString::from("-RF"), OsString::from("/opt/quire")]
    );
    assert_eq!(
        calls[1].1,
        vec![OsString::from("-F"), OsString::from("/opt/quire")]
    );
}
