use crate::support::{Accept, Recipe, Sandbox, build, read, v};
use slipway::apply::{ApplyError, ExitCode, rollback};

#[test]
fn rollback_swaps_back_and_forth_and_moves_the_links() {
    let sandbox = Sandbox::new();
    let mut first = Recipe::new("0.1.0");
    first.listed[0] = ("bin/casement", "casement one");
    sandbox.install(&build(&first)).expect("install");
    let mut second = Recipe::new("0.2.0");
    second.listed[0] = ("bin/casement", "casement two");
    sandbox.install(&build(&second)).expect("install");
    assert_eq!(
        read(&sandbox.layout.current().join("bin/casement")),
        "casement two"
    );

    let back = rollback(&sandbox.layout, &Accept).expect("rollback");
    assert_eq!(back.version, v("0.1.0"));
    assert_eq!(sandbox.current(), Some(v("0.1.0")));
    assert_eq!(sandbox.previous(), Some(v("0.2.0")));
    assert_eq!(
        read(&sandbox.layout.current().join("bin/casement")),
        "casement one"
    );
    assert_eq!(
        read(&sandbox.link("/usr/share/wayland-sessions/quire.desktop")),
        "[Desktop Entry]"
    );

    let forward = rollback(&sandbox.layout, &Accept).expect("rollback again");
    assert_eq!(forward.version, v("0.2.0"));
    assert_eq!(
        read(&sandbox.layout.current().join("bin/casement")),
        "casement two"
    );
}

#[test]
fn rollback_with_nothing_before_is_refused() {
    let sandbox = Sandbox::new();
    let none = rollback(&sandbox.layout, &Accept).expect_err("nothing installed");
    assert!(matches!(none, ApplyError::Plan(_)));
    sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    let only = rollback(&sandbox.layout, &Accept).expect_err("no previous");
    assert_eq!(only.exit_code(), ExitCode::Refused);
    assert_eq!(sandbox.current(), Some(v("0.1.0")));
}

#[test]
fn rollback_moves_the_exports_of_the_release_it_returns_to() {
    let sandbox = Sandbox::new();
    let mut first = Recipe::new("0.1.0");
    first.exports = vec![("/usr/local/bin/casement", "bin/casement")];
    sandbox.install(&build(&first)).expect("install");
    let mut second = Recipe::new("0.2.0");
    second.exports = vec![(
        "/usr/share/wayland-sessions/quire.desktop",
        "share/wayland-sessions/quire.desktop",
    )];
    sandbox.install(&build(&second)).expect("install");
    assert!(
        sandbox
            .link("/usr/share/wayland-sessions/quire.desktop")
            .exists()
    );
    assert!(std::fs::symlink_metadata(sandbox.link("/usr/local/bin/casement")).is_err());

    rollback(&sandbox.layout, &Accept).expect("rollback");
    assert!(sandbox.link("/usr/local/bin/casement").exists());
    assert!(
        std::fs::symlink_metadata(sandbox.link("/usr/share/wayland-sessions/quire.desktop"))
            .is_err()
    );
}
