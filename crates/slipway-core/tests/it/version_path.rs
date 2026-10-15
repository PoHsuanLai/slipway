use semver::Version;
use slipway_core::{BundlePath, PathError, Standing, standing};

fn v(text: &str) -> Version {
    Version::parse(text).expect("version")
}

#[test]
fn only_a_newer_version_is_ever_offered() {
    let table = [
        ("0.1.0", "0.2.0", Standing::Newer),
        ("0.1.9", "0.1.10", Standing::Newer),
        ("1.0.0-rc.1", "1.0.0", Standing::Newer),
        ("0.2.0", "0.2.0", Standing::Same),
        ("0.2.0", "0.2.0+build5", Standing::Same),
        ("0.2.0", "0.1.9", Standing::Older),
        ("1.0.0", "1.0.0-rc.1", Standing::Older),
        ("0.10.0", "0.9.0", Standing::Older),
    ];
    for (installed, offered, expected) in table {
        assert_eq!(
            standing(&v(installed), &v(offered)),
            expected,
            "{installed} -> {offered}"
        );
    }
}

#[test]
fn bundle_paths_table() {
    let ok = [
        "bin/casement",
        "a",
        "share/x y/z.desktop",
        "lib/libfoo.so.1",
        ".hidden/file",
    ];
    for path in ok {
        assert!(BundlePath::parse(path).is_ok(), "{path}");
    }
    let long = "a/".repeat(600);
    let bad: [(&str, fn(&PathError) -> bool); 11] = [
        ("", |e| matches!(e, PathError::Empty)),
        ("/etc/passwd", |e| matches!(e, PathError::Absolute(_))),
        ("../x", |e| matches!(e, PathError::Climbing(_))),
        ("a/../b", |e| matches!(e, PathError::Climbing(_))),
        ("a/./b", |e| matches!(e, PathError::Climbing(_))),
        ("a//b", |e| matches!(e, PathError::Climbing(_))),
        ("a/", |e| matches!(e, PathError::Climbing(_))),
        ("a\\b", |e| matches!(e, PathError::BadCharacter(_))),
        ("a\nb", |e| matches!(e, PathError::BadCharacter(_))),
        (".slipway/x", |e| matches!(e, PathError::Reserved(_))),
        (long.as_str(), |e| matches!(e, PathError::TooLong)),
    ];
    for (path, expect) in bad {
        let err = BundlePath::parse(path).expect_err(path);
        assert!(expect(&err), "{path}: {err:?}");
    }
}

#[test]
fn ancestors_are_listed_shortest_first() {
    let path = BundlePath::parse("a/b/c").expect("path");
    assert_eq!(path.ancestors(), vec!["a", "a/b"]);
    assert!(BundlePath::parse("a").expect("path").ancestors().is_empty());
}
