use crate::support::{Recipe, Sandbox, build, read};
use std::os::unix::fs::symlink;

const SESSION: &str = "/usr/share/wayland-sessions/quire.desktop";

fn is_link(path: &std::path::Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

#[test]
fn links_follow_the_release_added_kept_and_dropped() {
    let sandbox = Sandbox::new();
    let mut first = Recipe::new("0.1.0");
    first.exports = vec![
        (SESSION, "share/wayland-sessions/quire.desktop"),
        ("/usr/local/bin/casement", "bin/casement"),
    ];
    sandbox.install(&build(&first)).expect("install");
    assert!(is_link(&sandbox.link(SESSION)));
    assert!(is_link(&sandbox.link("/usr/local/bin/casement")));

    let mut second = Recipe::new("0.2.0");
    second.exports = vec![
        (SESSION, "share/wayland-sessions/quire.desktop"),
        ("/usr/local/bin/other", "bin/casement"),
    ];
    sandbox.install(&build(&second)).expect("install");
    assert!(is_link(&sandbox.link(SESSION)));
    assert!(is_link(&sandbox.link("/usr/local/bin/other")));
    assert!(
        !is_link(&sandbox.link("/usr/local/bin/casement")),
        "a link the new release no longer lists is removed"
    );
}

#[test]
fn a_regular_file_in_the_way_is_never_replaced_and_is_reported() {
    let sandbox = Sandbox::new();
    let place = sandbox.link(SESSION);
    std::fs::create_dir_all(place.parent().expect("parent")).expect("dirs");
    std::fs::write(&place, "the person's own file").expect("file");
    let applied = sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install still succeeds");
    assert_eq!(applied.conflicts, vec![SESSION.to_owned()]);
    assert_eq!(read(&place), "the person's own file");
    assert!(!is_link(&place));
}

#[test]
fn a_link_to_somewhere_else_is_left_alone_but_one_into_the_root_is_replaced() {
    let sandbox = Sandbox::new();
    let place = sandbox.link(SESSION);
    std::fs::create_dir_all(place.parent().expect("parent")).expect("dirs");
    symlink("/somewhere/else", &place).expect("symlink");
    let applied = sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    assert_eq!(applied.conflicts, vec![SESSION.to_owned()]);
    assert_eq!(
        std::fs::read_link(&place).expect("link"),
        std::path::PathBuf::from("/somewhere/else")
    );

    std::fs::remove_file(&place).expect("remove");
    symlink(
        sandbox.layout.root.join("current/old/place.desktop"),
        &place,
    )
    .expect("symlink");
    let applied = sandbox
        .install(&build(&Recipe::new("0.2.0")))
        .expect("install");
    assert!(applied.conflicts.is_empty());
    assert_eq!(read(&place), "[Desktop Entry]");
}

#[test]
fn installing_again_is_the_same_as_installing_once() {
    let sandbox = Sandbox::new();
    sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    let before = std::fs::read_link(sandbox.link(SESSION)).expect("link");
    sandbox
        .install(&build(&Recipe::new("0.2.0")))
        .expect("install");
    assert_eq!(
        std::fs::read_link(sandbox.link(SESSION)).expect("link"),
        before
    );
}
