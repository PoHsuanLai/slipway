use crate::support::{Accept, Recipe, Sandbox, build, read, v};
use slipway_root::apply::{
    Change, Command, Ran, Reach, Relabel, Relabeled, RunProgram, Selinux, Settling, commands,
    install_with, rollback,
};
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

const UNIT: &str = "/usr/lib/systemd/user/sill.service";
const UNIT_IN_BUNDLE: &str = "system/usr/lib/systemd/user/sill.service";
const UNIT_IN_TREE: &str = "lib/systemd/user/sill.service";

#[test]
fn a_unit_that_was_an_export_link_becomes_a_plain_file_and_the_old_release_is_untouched() {
    let sandbox = Sandbox::new();
    let at = sandbox.link(UNIT);

    // 0.1.0 exports the unit as a link into the install root, as releases did.
    let mut first = Recipe::new("0.1.0");
    first.listed.push((UNIT_IN_TREE, "unit v1"));
    first.exports.push((UNIT, UNIT_IN_TREE));
    sandbox.install(&build(&first)).expect("install");
    assert!(
        std::fs::symlink_metadata(&at)
            .expect("meta")
            .file_type()
            .is_symlink()
    );

    // 0.2.0 places it as a system file instead, so the unit manager reads a root-owned file.
    let mut second = Recipe::new("0.2.0");
    second.listed.push((UNIT_IN_BUNDLE, "unit v2"));
    second.system.push(UNIT);
    let applied = sandbox.install(&build(&second)).expect("install");
    assert!(applied.conflicts.is_empty(), "{:?}", applied.conflicts);
    let meta = std::fs::symlink_metadata(&at).expect("meta");
    assert!(meta.file_type().is_file(), "a plain file, not a link");
    assert_eq!(read(&at), "unit v2");
    assert_eq!(meta.permissions().mode() & 0o777, 0o644);
    assert_eq!(
        read(&sandbox.layout.release_dir(&v("0.1.0")).join(UNIT_IN_TREE)),
        "unit v1",
        "the link was replaced, never written through"
    );
}

#[test]
fn a_stale_link_into_the_install_root_is_replaced_and_any_other_link_is_not() {
    let into_root = |sandbox: &Sandbox| {
        sandbox
            .layout
            .root
            .join("an-old-release")
            .join(UNIT_IN_TREE)
    };
    let elsewhere = |sandbox: &Sandbox| sandbox.tmp.path().join("elsewhere");
    // (case, where the link points, replaced)
    type Target = fn(&Sandbox) -> PathBuf;
    let table: [(&str, Target, bool); 3] = [
        ("into the install root", into_root, true),
        ("somewhere else", elsewhere, false),
        (
            "climbing out of the install root",
            |s| s.layout.root.join("../elsewhere"),
            false,
        ),
    ];
    for (name, target, replaced) in table {
        let sandbox = Sandbox::new();
        let at = sandbox.link(UNIT);
        let target = target(&sandbox);
        std::fs::create_dir_all(target.parent().expect("parent")).expect("dirs");
        std::fs::write(&target, "what the link points at").expect("file");
        std::fs::create_dir_all(at.parent().expect("parent")).expect("dirs");
        symlink(&target, &at).expect("symlink");

        let mut recipe = Recipe::new("0.1.0");
        recipe.listed.push((UNIT_IN_BUNDLE, "unit"));
        recipe.system.push(UNIT);
        let applied = sandbox.install(&build(&recipe)).expect("install");

        assert_eq!(
            read(&target),
            "what the link points at",
            "{name}: not followed"
        );
        let is_link = std::fs::symlink_metadata(&at)
            .expect("meta")
            .file_type()
            .is_symlink();
        assert_eq!(!is_link, replaced, "{name}");
        assert_eq!(applied.conflicts.is_empty(), replaced, "{name}");
    }
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

    let back = rollback(&sandbox.layout, &Accept).expect("rollback");
    assert_eq!(back.version, v("0.1.0"));
    assert_eq!(read(&at), "v1");
    assert!(
        !pam.exists(),
        "a file only the release we left had is removed"
    );
}

#[test]
fn a_release_listing_a_system_file_no_rule_names_is_refused_whole() {
    // (destination) a folder the helper knows, a name it does not; and folders it does not know at all.
    let table = [
        "/etc/shadow",
        "/etc/polkit-1/rules.d/00-all.rules",
        "/etc/pam.d/sudo",
        "/etc/udev/rules.d/99-evil.rules",
        "/usr/lib/systemd/system/evil.service",
        "/usr/lib/systemd/system/turnstile.timer",
        "/etc/systemd/system/turnstile.socket",
        "/etc/udev/rules.d/73-turnstile.rules",
        "/etc/modules-load.d/turnstile.d.conf",
        "/usr/lib/systemd/user/sill.target",
        "/usr/lib/systemd/user/xdg-desktop-portal.service.d/20-evil.conf",
        "/usr/lib/systemd/system/palmrest.service.d",
    ];
    for dest in table {
        let sandbox = Sandbox::new();
        let in_bundle = format!("system{dest}");
        let mut recipe = Recipe::new("0.1.0");
        recipe.listed.push((&in_bundle, "contents"));
        recipe.system.push(dest);
        sandbox.install(&build(&recipe)).expect_err(dest);
        assert_eq!(sandbox.current(), None, "{dest}");
        assert!(!sandbox.link(dest).exists(), "{dest}");
    }
}

#[test]
fn a_file_the_distribution_ships_in_usr_lib_is_not_replaced_by_a_new_one_in_etc() {
    // On Fedora and Arch the vendor's PAM and udev files live under /usr/lib, so the /etc name is empty.
    let table = [
        (PAM, "/usr/lib/pam.d/sill", PAM_IN_BUNDLE),
        (
            UDEV,
            "/usr/lib/udev/rules.d/70-keycap.rules",
            UDEV_IN_BUNDLE,
        ),
    ];
    for (dest, vendor, in_bundle) in table {
        let sandbox = Sandbox::new();
        let vendor_file = sandbox.link(vendor);
        std::fs::create_dir_all(vendor_file.parent().expect("parent")).expect("dirs");
        std::fs::write(&vendor_file, "the vendor's").expect("vendor file");
        let mut recipe = Recipe::new("0.1.0");
        recipe.listed.push((in_bundle, "ours"));
        recipe.system.push(dest);

        let applied = sandbox.install(&build(&recipe)).expect("install");
        assert_eq!(applied.conflicts, vec![dest.to_owned()], "{dest}");
        assert!(!sandbox.link(dest).exists(), "{dest}: nothing was placed");
        assert_eq!(read(&vendor_file), "the vendor's");
    }
}

#[test]
fn a_system_file_that_cannot_be_placed_leaves_the_earlier_release_current() {
    let sandbox = Sandbox::new();
    sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    // /etc/udev is a file here, so nothing can be placed below it: the failure comes after the switch.
    let etc = sandbox.layout.export_base.join("etc");
    std::fs::create_dir_all(&etc).expect("etc");
    std::fs::write(etc.join("udev"), "in the way").expect("file");

    let err = sandbox
        .install(&build(&placing("0.2.0", "v2")))
        .expect_err("cannot place");
    match &err {
        slipway_root::apply::ApplyError::AfterSwitch { back, .. } => {
            assert_eq!(*back, slipway_root::apply::Back::Restored, "{err}");
        }
        other => panic!("wrong error: {other}"),
    }
    assert_eq!(sandbox.current(), Some(v("0.1.0")));
    assert_eq!(sandbox.previous(), None);
    assert_eq!(
        read(&sandbox.link("/usr/share/wayland-sessions/quire.desktop")),
        "[Desktop Entry]",
        "the links follow the release that is current"
    );
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

const INPUT_FILES: [&str; 4] = [
    "/usr/lib/systemd/system/turnstile.socket",
    "/usr/lib/systemd/system/turnstile.service",
    "/etc/udev/rules.d/72-turnstile.rules",
    "/etc/modules-load.d/turnstile.conf",
];

#[test]
fn each_file_of_the_input_service_is_placed_and_the_system_is_told_once() {
    let sandbox = Sandbox::new();
    let bundled: Vec<String> = INPUT_FILES.iter().map(|d| format!("system{d}")).collect();
    let mut recipe = Recipe::new("0.1.0");
    for (in_bundle, dest) in bundled.iter().zip(INPUT_FILES) {
        recipe.listed.push((in_bundle, dest));
        recipe.system.push(dest);
    }
    let fake = Fake::new(vec![
        ("/usr/bin/systemctl", Ran::Succeeded),
        ("/usr/bin/udevadm", Ran::Succeeded),
    ]);
    let host = Settling::new(slipway_root::apply::NoRelabel, &fake);
    let applied = install_with(
        &sandbox.layout,
        &sandbox.stage(&build(&recipe)),
        &Accept,
        &host,
    )
    .expect("install");
    assert!(applied.conflicts.is_empty() && applied.warnings.is_empty());
    for dest in INPUT_FILES {
        assert_eq!(read(&sandbox.link(dest)), dest, "{dest}");
    }
    let calls: Vec<(String, Vec<String>)> = fake
        .calls
        .borrow()
        .iter()
        .map(|(program, args)| {
            (
                program.display().to_string(),
                args.iter()
                    .map(|a| a.to_string_lossy().into_owned())
                    .collect(),
            )
        })
        .collect();
    let want = |program: &str, args: &[&str]| {
        (
            program.to_owned(),
            args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>(),
        )
    };
    assert_eq!(
        calls,
        vec![
            want("/usr/bin/systemctl", &["daemon-reload"]),
            want("/usr/bin/udevadm", &["control", "--reload-rules"]),
            want("/usr/bin/udevadm", &["trigger", "--subsystem-match=misc"]),
            want(
                "/usr/bin/systemctl",
                &["enable", "--now", "turnstile.socket"]
            ),
        ]
    );
}

#[test]
fn a_failing_or_missing_tool_is_a_note_and_never_stops_an_install() {
    let mut recipe = Recipe::new("0.1.0");
    let dest = "/usr/lib/systemd/system/turnstile.socket";
    recipe
        .listed
        .push(("system/usr/lib/systemd/system/turnstile.socket", "socket"));
    recipe.system.push(dest);

    // (what the tools answer, notes expected)
    let table: [(Vec<(&str, Ran)>, usize); 3] = [
        (vec![], 0),
        (vec![("/usr/bin/systemctl", Ran::Failed("no".into()))], 2),
        (vec![("/bin/systemctl", Ran::Failed("no".into()))], 2),
    ];
    for (answers, notes) in table {
        let sandbox = Sandbox::new();
        let fake = Fake::new(answers);
        let host = Settling::new(slipway_root::apply::NoRelabel, &fake);
        let applied = install_with(
            &sandbox.layout,
            &sandbox.stage(&build(&recipe)),
            &Accept,
            &host,
        )
        .expect("install");
        assert_eq!(applied.warnings.len(), notes);
        assert_eq!(read(&sandbox.link(dest)), "socket");
    }
}

#[test]
fn what_the_system_is_told_follows_the_paths_that_changed() {
    let placed = |p: &str| Change::Placed(p.to_owned());
    let removed = |p: &str| Change::Removed(p.to_owned());
    let args = |c: &Command| c.args.join(" ");
    // (case, changes, the commands as text)
    let table: Vec<(&str, Vec<Change>, Vec<&str>)> = vec![
        ("nothing changed", vec![], vec![]),
        ("a login service", vec![placed("/etc/pam.d/sill")], vec![]),
        (
            "a unit",
            vec![placed("/usr/lib/systemd/system/other.service")],
            vec!["daemon-reload"],
        ),
        (
            "the keyboard map is placed: read again, then enabled and started",
            vec![placed("/usr/lib/systemd/system/keycap.service")],
            vec!["daemon-reload", "enable --now keycap.service"],
        ),
        (
            "the keyboard map is removed: read again, not started",
            vec![removed("/usr/lib/systemd/system/keycap.service")],
            vec!["daemon-reload"],
        ),
        (
            "both services: the socket first, the map after it",
            vec![
                placed("/usr/lib/systemd/system/keycap.service"),
                placed("/usr/lib/systemd/system/turnstile.socket"),
            ],
            vec![
                "daemon-reload",
                "enable --now turnstile.socket",
                "enable --now keycap.service",
            ],
        ),
        (
            "a rule",
            vec![placed("/etc/udev/rules.d/70-keycap.rules")],
            vec!["control --reload-rules", "trigger --subsystem-match=misc"],
        ),
        (
            "the socket is placed",
            vec![placed("/usr/lib/systemd/system/turnstile.socket")],
            vec!["daemon-reload", "enable --now turnstile.socket"],
        ),
        (
            "the socket is removed: read again, not started",
            vec![removed("/usr/lib/systemd/system/turnstile.socket")],
            vec!["daemon-reload"],
        ),
        (
            "the service alone is not started",
            vec![placed("/usr/lib/systemd/system/turnstile.service")],
            vec!["daemon-reload"],
        ),
    ];
    for (case, changes, want) in table {
        let got: Vec<String> = commands(&changes).iter().map(args).collect();
        assert_eq!(got, want, "{case}");
    }
}
