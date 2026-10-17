use crate::support::{Doc, f};
use slipway_core::release::{ALLOWED_SYSTEM_FILES, NameRule, is_allowed_system_path};
use slipway_core::{
    Placement, Release, ReleaseError, Sha256Hex, SystemAction, SystemFound, placements,
    plan_system, vendor_copies,
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
fn a_system_file_is_accepted_only_when_a_rule_names_it() {
    // (destination, accepted)
    let table: [(&str, bool); 65] = [
        // The files the desktop places today.
        ("/etc/pam.d/sill", true),
        ("/etc/udev/rules.d/60-sill-ddc.rules", true),
        ("/etc/udev/rules.d/70-keycap.rules", true),
        ("/etc/udev/rules.d/70-palmrest.rules", true),
        ("/etc/modules-load.d/sill-ddc.conf", true),
        ("/etc/modules-load.d/keycap.conf", true),
        ("/usr/lib/keycap/keycap.toml", true),
        ("/etc/modprobe.d/99-palmrest.conf", true),
        ("/usr/lib/systemd/system/keycap.service", true),
        ("/etc/polkit-1/rules.d/keycap.polkit.rules", true),
        ("/usr/lib/systemd/system/turnstile.socket", true),
        ("/usr/lib/systemd/system/turnstile.service", true),
        ("/etc/udev/rules.d/72-turnstile.rules", true),
        ("/etc/modules-load.d/turnstile.conf", true),
        // The touchpad daemon as a system service, and the user units that must be plain files.
        ("/usr/lib/systemd/system/palmrest.service", true),
        ("/usr/lib/systemd/user/sill.service", true),
        ("/usr/lib/systemd/user/org.quire.Settings.service", true),
        (
            "/usr/lib/systemd/user/xdg-desktop-portal.service.d/10-quire.conf",
            true,
        ),
        ("/etc/porter/callers.toml", true),
        ("/etc/porter/inferd-callers.toml", true),
        ("/etc/quire/intentd-callers.toml", true),
        ("/etc/quire/memory-callers.toml", true),
        ("/etc/xdg/quire/docket.settings.toml", true),
        // A folder that is listed but a name nobody named: the point of a per-file list.
        ("/etc/pam.d/sudo", false),
        ("/etc/pam.d/system-auth", false),
        ("/etc/polkit-1/rules.d/00-all.rules", false),
        ("/etc/polkit-1/rules.d/49-keycap.rules", false),
        ("/etc/udev/rules.d/99-evil.rules", false),
        ("/etc/modules-load.d/evil.conf", false),
        ("/etc/modprobe.d/blacklist.conf", false),
        ("/usr/lib/systemd/system/evil.service", false),
        ("/usr/lib/systemd/system/turnstile.timer", false),
        ("/usr/lib/systemd/system/turnstile.service.d", false),
        ("/etc/udev/rules.d/73-turnstile.rules", false),
        ("/usr/lib/systemd/system/palmrest.socket", false),
        ("/usr/lib/systemd/user/sill.target", false),
        ("/usr/lib/systemd/user/.service", false),
        ("/usr/lib/systemd/user/sill.service.d/10-evil.conf", false),
        (
            "/usr/lib/systemd/user/xdg-desktop-portal.service.d/20-evil.conf",
            false,
        ),
        ("/usr/lib/systemd/user/other.service.d/10-quire.conf", false),
        ("/usr/lib/systemd/user/sub/sill.service", false),
        ("/etc/porter/other.toml", false),
        ("/etc/quire/policy.toml", false),
        ("/etc/quire/-callers.toml", false),
        ("/etc/xdg/quire/evil.sh", false),
        // Not a listed folder.
        ("/etc/passwd", false),
        ("/etc/sudoers.d/quire", false),
        ("/etc/systemd/system/keycap.service", false),
        ("/etc/systemd/system/turnstile.socket", false),
        ("/usr/lib/udev/rules.d/70-keycap.rules", false),
        ("/usr/bin/keycap", false),
        // The administrator's keycap folder is never placed into; only the vendor folder is.
        ("/etc/keycap/keycap.toml", false),
        ("/usr/lib/keycap/other.toml", false),
        // Security profiles are never placed by an update (owner, 2026-10-16).
        ("/etc/apparmor.d/bwrap", false),
        ("/etc/apparmor.d/usr.bin.bwrap", false),
        // Beside or below a listed folder.
        ("/etc/udev/rules.d2/70-keycap.rules", false),
        ("/etc/pam.d/sub/sill", false),
        ("/etc/xdg/quire/sub/x.toml", false),
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
        assert_eq!(is_allowed_system_path(dest), accepted, "{dest}");
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
fn the_allowed_system_files_are_listed_by_name() {
    for rule in ALLOWED_SYSTEM_FILES {
        assert!(
            rule.dir.starts_with("/etc/") || rule.dir.starts_with("/usr/lib/"),
            "{}",
            rule.dir
        );
        assert!(
            !rule.dir.ends_with('/') && !rule.dir.contains('*'),
            "{}",
            rule.dir
        );
        let text = match rule.name {
            NameRule::Exact(name) | NameRule::Ending(name) => name,
        };
        assert!(!text.contains('/') && !text.contains('*'), "{text}");
    }
    let unique: std::collections::BTreeSet<_> = ALLOWED_SYSTEM_FILES
        .iter()
        .map(|r| (r.dir, format!("{:?}", r.name)))
        .collect();
    assert_eq!(unique.len(), ALLOWED_SYSTEM_FILES.len());
}

#[test]
fn a_vendor_copy_is_found_where_the_distribution_keeps_it() {
    let table: [(&str, &[&str]); 7] = [
        ("/etc/pam.d/sill", &["/usr/lib/pam.d/sill"]),
        (
            "/etc/udev/rules.d/70-keycap.rules",
            &["/usr/lib/udev/rules.d/70-keycap.rules"],
        ),
        (
            "/etc/modules-load.d/keycap.conf",
            &["/usr/lib/modules-load.d/keycap.conf"],
        ),
        (
            "/etc/modprobe.d/99-palmrest.conf",
            &["/usr/lib/modprobe.d/99-palmrest.conf"],
        ),
        (
            "/etc/polkit-1/rules.d/keycap.polkit.rules",
            &["/usr/share/polkit-1/rules.d/keycap.polkit.rules"],
        ),
        ("/usr/lib/systemd/system/keycap.service", &[]),
        ("/etc/quire/intentd-callers.toml", &[]),
    ];
    for (path, want) in table {
        assert_eq!(vendor_copies(path), want, "{path}");
    }
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
            "a path both linked and placed",
            |d| {
                let unit = "/usr/lib/systemd/user/sill.service";
                d.files.push(f(&format!("system{unit}"), "unit"));
                d.system.push(unit.to_owned());
                d.exports
                    .push((unit.into(), "share/wayland-sessions/quire.desktop".into()));
            },
            |e| matches!(e, ReleaseError::ExportedAndPlaced(_)),
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
            "an earlier release's export link where this one places a file: replace",
            plan(
                std::slice::from_ref(&new_v2),
                &[],
                &[(UDEV, SystemFound::ExportLink)],
            ),
            vec![place(&new_v2)],
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
