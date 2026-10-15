use crate::support::{Doc, f};
use slipway_core::{
    Placement, Release, ReleaseError, Sha256Hex, SystemAction, SystemFound, placements, plan_system,
};
use std::collections::BTreeMap;

const UDEV: &str = "/etc/udev/rules.d/70-keycap.rules";

/// A release that places `dest`, with its contents in the bundle where they belong.
fn doc_placing(dest: &str) -> Doc {
    let mut doc = Doc::good();
    doc.files.push(f(&format!("system{dest}"), "contents"));
    doc.system.push(dest.to_owned());
    doc
}

#[test]
fn a_system_file_is_accepted_only_directly_in_an_allowed_folder() {
    // (destination, accepted)
    let table: [(&str, bool); 27] = [
        ("/etc/udev/rules.d/70-keycap.rules", true),
        ("/etc/modules-load.d/keycap.conf", true),
        ("/etc/modprobe.d/99-palmrest.conf", true),
        ("/usr/lib/systemd/system/keycap.service", true),
        ("/etc/pam.d/sill", true),
        ("/etc/polkit-1/rules.d/49-keycap.rules", true),
        ("/etc/porter/callers.toml", true),
        ("/etc/quire/policy.toml", true),
        ("/etc/udev/rules.d/a@b+c_d-e.rules", true),
        // Not a listed folder.
        ("/etc/passwd", false),
        ("/etc/shadow", false),
        ("/etc/sudoers.d/quire", false),
        ("/etc/systemd/system/keycap.service", false),
        ("/usr/lib/udev/rules.d/70-keycap.rules", false),
        ("/usr/bin/keycap", false),
        ("/boot/vmlinuz", false),
        ("/root/.ssh/authorized_keys", false),
        // Beside or below a listed folder.
        ("/etc/udev/rules.d2/x.rules", false),
        ("/etc/pam.d/sub/sill", false),
        ("/etc/quire2/x", false),
        // The folder itself, climbing, doubled slashes, hidden names, odd characters.
        ("/etc/pam.d", false),
        ("/etc/pam.d/", false),
        ("/etc/pam.d/../shadow", false),
        ("/etc/pam.d//sill", false),
        ("/etc/pam.d/.sill", false),
        ("/etc/pam.d/si ll", false),
        ("etc/pam.d/sill", false),
    ];
    for (dest, accepted) in table {
        let parsed = Release::parse(doc_placing(dest).render().as_bytes());
        match (accepted, parsed) {
            (true, Ok(release)) => assert_eq!(release.system.len(), 1, "{dest}"),
            // A destination that is not even a clean path may be refused as it is read.
            (
                false,
                Err(
                    ReleaseError::BadSystemPath(_)
                    | ReleaseError::Path(_)
                    | ReleaseError::Unreadable(_),
                ),
            ) => {}
            (true, Err(e)) => panic!("{dest}: refused: {e:?}"),
            (false, Ok(_)) => panic!("{dest}: accepted"),
            (false, Err(e)) => panic!("{dest}: wrong refusal {e:?}"),
        }
    }
}

#[test]
fn the_allowed_system_folders_are_listed_by_name() {
    let dirs = slipway_core::release::ALLOWED_SYSTEM_DIRS;
    for dir in dirs {
        assert!(
            dir.starts_with("/etc/") || dir.starts_with("/usr/lib/"),
            "{dir}"
        );
        assert!(!dir.ends_with('/') && !dir.contains('*'), "{dir}");
    }
    let unique: std::collections::BTreeSet<_> = dirs.iter().collect();
    assert_eq!(unique.len(), dirs.len());
}

type Mutate = fn(&mut Doc);
type Expect = fn(&ReleaseError) -> bool;

#[test]
fn a_system_listing_that_does_not_match_the_bundle_is_refused() {
    let table: Vec<(&str, Mutate, Expect)> = vec![
        (
            "listed twice",
            |d| d.system.push(UDEV.into()),
            |e| matches!(e, ReleaseError::DuplicateSystemFile(_)),
        ),
        (
            "contents not in the bundle",
            |d| d.files.retain(|f| !f.path.starts_with("system/")),
            |e| matches!(e, ReleaseError::SystemSourceMissing(_)),
        ),
        (
            "a file in the system folder nobody listed",
            |d| d.files.push(f("system/etc/pam.d/other", "x")),
            |e| matches!(e, ReleaseError::SystemFileUnlisted(_)),
        ),
        (
            "a listed file outside the allowed folders in the system folder",
            |d| d.files.push(f("system/etc/shadow", "x")),
            |e| matches!(e, ReleaseError::SystemFileUnlisted(_)),
        ),
    ];
    for (name, mutate, expect) in table {
        let mut doc = doc_placing(UDEV);
        mutate(&mut doc);
        match Release::parse(doc.render().as_bytes()) {
            Ok(_) => panic!("{name}: accepted"),
            Err(e) => assert!(expect(&e), "{name}: wrong refusal {e:?}"),
        }
    }
}

#[test]
fn a_release_without_system_files_has_none() {
    let release = Release::parse(Doc::good().render().as_bytes()).expect("parses");
    assert!(release.system.is_empty());
    assert_eq!(placements(&release).expect("placements"), Vec::new());
}

#[test]
fn placements_carry_the_listed_digest_and_mode() {
    let release = Release::parse(doc_placing(UDEV).render().as_bytes()).expect("parses");
    let got = placements(&release).expect("placements");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].path, UDEV);
    assert_eq!(got[0].source.as_str(), format!("system{UDEV}"));
    assert_eq!(got[0].sha256, Sha256Hex::of(b"contents"));
    assert_eq!(got[0].mode, 0o644);
}

fn placement(path: &str, content: &str) -> Placement {
    Placement {
        path: path.to_owned(),
        source: slipway_core::BundlePath::parse(&format!("system{path}")).expect("source"),
        sha256: Sha256Hex::of(content.as_bytes()),
        mode: 0o644,
    }
}

fn file(content: &str) -> SystemFound {
    SystemFound::File(Sha256Hex::of(content.as_bytes()))
}

fn plan(new: &[Placement], old: &[Placement], seen: &[(&str, SystemFound)]) -> Vec<SystemAction> {
    let found: BTreeMap<String, SystemFound> = seen
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect();
    plan_system(new, old, &found)
}

#[test]
fn system_files_are_placed_kept_replaced_removed_or_left_alone() {
    let new_v2 = placement(UDEV, "v2");
    let old_v1 = placement(UDEV, "v1");
    let gone = placement("/etc/pam.d/sill", "pam");
    let place = |p: &Placement| SystemAction::Place(p.clone());
    let conflict = |p: &str| SystemAction::Conflict { path: p.to_owned() };

    // (case, planned, expected)
    let cases: Vec<(&str, Vec<SystemAction>, Vec<SystemAction>)> = vec![
        (
            "nothing there: place",
            plan(std::slice::from_ref(&new_v2), &[], &[]),
            vec![place(&new_v2)],
        ),
        (
            "already the right contents: keep",
            plan(std::slice::from_ref(&new_v2), &[], &[(UDEV, file("v2"))]),
            vec![SystemAction::Keep { path: UDEV.into() }],
        ),
        (
            "the earlier release's contents: replace",
            plan(
                std::slice::from_ref(&new_v2),
                std::slice::from_ref(&old_v1),
                &[(UDEV, file("v1"))],
            ),
            vec![place(&new_v2)],
        ),
        (
            "the person's own contents: leave alone",
            plan(
                std::slice::from_ref(&new_v2),
                std::slice::from_ref(&old_v1),
                &[(UDEV, file("mine"))],
            ),
            vec![conflict(UDEV)],
        ),
        (
            "contents of an earlier release we did not place: leave alone",
            plan(std::slice::from_ref(&new_v2), &[], &[(UDEV, file("v1"))]),
            vec![conflict(UDEV)],
        ),
        (
            "a link or folder in the way: leave alone",
            plan(
                std::slice::from_ref(&new_v2),
                std::slice::from_ref(&old_v1),
                &[(UDEV, SystemFound::Foreign)],
            ),
            vec![conflict(UDEV)],
        ),
        (
            "dropped by the new release and untouched: remove",
            plan(
                &[],
                std::slice::from_ref(&gone),
                &[("/etc/pam.d/sill", file("pam"))],
            ),
            vec![SystemAction::Remove {
                path: "/etc/pam.d/sill".into(),
            }],
        ),
        (
            "dropped by the new release but edited: leave alone, say nothing",
            plan(
                &[],
                std::slice::from_ref(&gone),
                &[("/etc/pam.d/sill", file("edited"))],
            ),
            vec![],
        ),
        (
            "dropped by the new release and already gone",
            plan(&[], std::slice::from_ref(&gone), &[]),
            vec![],
        ),
        (
            "dropped but replaced by a link: leave alone",
            plan(&[], &[gone], &[("/etc/pam.d/sill", SystemFound::Foreign)]),
            vec![],
        ),
    ];
    for (name, got, want) in cases {
        assert_eq!(got, want, "{name}");
    }
}
