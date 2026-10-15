use slipway_core::{BundlePath, Export, ExportAction, Found, plan_exports};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const ROOT: &str = "/opt/quire";
const LINK: &str = "/usr/share/wayland-sessions/quire.desktop";

fn export(link: &str, target: &str) -> Export {
    Export {
        link: link.to_owned(),
        target: BundlePath::parse(target).expect("target"),
    }
}

fn to(target: &str) -> PathBuf {
    Path::new(ROOT).join("current").join(target)
}

fn found(entries: &[(&str, Found)]) -> BTreeMap<String, Found> {
    entries
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

fn plan(new: &[Export], old: &[Export], seen: &[(&str, Found)]) -> Vec<ExportAction> {
    plan_exports(Path::new(ROOT), new, old, &found(seen))
}

#[test]
fn a_missing_link_is_created_pointing_through_current() {
    let new = [export(LINK, "share/q.desktop")];
    assert_eq!(
        plan(&new, &[], &[]),
        vec![ExportAction::Create {
            link: LINK.into(),
            to: to("share/q.desktop")
        }]
    );
}

#[test]
fn a_link_that_is_already_right_is_kept() {
    let new = [export(LINK, "share/q.desktop")];
    let seen = [(LINK, Found::Link(to("share/q.desktop")))];
    assert_eq!(
        plan(&new, &new, &seen),
        vec![ExportAction::Keep { link: LINK.into() }]
    );
}

#[test]
fn a_stale_link_into_the_root_is_replaced() {
    let new = [export(LINK, "share/q.desktop")];
    let seen = [(LINK, Found::Link(to("old/q.desktop")))];
    assert_eq!(
        plan(&new, &[], &seen),
        vec![ExportAction::Replace {
            link: LINK.into(),
            to: to("share/q.desktop")
        }]
    );
}

#[test]
fn nothing_that_is_not_ours_is_ever_replaced() {
    let new = [export(LINK, "share/q.desktop")];
    let elsewhere = Found::Link(PathBuf::from("/usr/lib/other/q.desktop"));
    let climbing = Found::Link(PathBuf::from("/opt/quire/../etc/shadow"));
    let sibling = Found::Link(PathBuf::from("/opt/quire2/current/x"));
    for seen in [Found::Foreign, elsewhere, climbing, sibling] {
        assert_eq!(
            plan(&new, &[], &[(LINK, seen.clone())]),
            vec![ExportAction::Conflict { link: LINK.into() }],
            "{seen:?}"
        );
    }
}

#[test]
fn links_the_new_release_drops_are_removed_only_when_they_are_ours() {
    let old = [
        export("/usr/local/bin/a", "bin/a"),
        export("/usr/local/bin/b", "bin/b"),
        export("/usr/local/bin/c", "bin/c"),
    ];
    let new = [export("/usr/local/bin/a", "bin/a")];
    let seen = [
        ("/usr/local/bin/a", Found::Link(to("bin/a"))),
        ("/usr/local/bin/b", Found::Link(to("bin/b"))),
        (
            "/usr/local/bin/c",
            Found::Link(PathBuf::from("/somewhere/else")),
        ),
    ];
    assert_eq!(
        plan(&new, &old, &seen),
        vec![
            ExportAction::Keep {
                link: "/usr/local/bin/a".into()
            },
            ExportAction::Remove {
                link: "/usr/local/bin/b".into()
            },
        ]
    );
}

#[test]
fn a_dropped_link_that_is_already_gone_needs_nothing() {
    let old = [export("/usr/local/bin/b", "bin/b")];
    assert_eq!(plan(&[], &old, &[]), vec![]);
    assert_eq!(
        plan(&[], &old, &[("/usr/local/bin/b", Found::Foreign)]),
        vec![]
    );
}
