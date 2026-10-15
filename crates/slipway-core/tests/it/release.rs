use crate::support::{Doc, f};
use slipway_core::{PathError, Release, ReleaseError, Sha256Hex, UnixSeconds};

#[test]
fn a_good_release_parses_with_everything_in_it() {
    let release = Release::parse(Doc::good().render().as_bytes()).expect("parses");
    assert_eq!(release.version.to_string(), "0.2.0");
    assert_eq!(release.released, UnixSeconds(1_792_483_200));
    assert_eq!(release.notes, "Faster start.");
    assert_eq!(release.bundle.name, "bundle.tar.zst");
    assert_eq!(release.bundle.size, 123);
    assert_eq!(release.components["casement"], "e616a79");
    assert_eq!(release.files.len(), 2);
    assert_eq!(release.file("bin/casement").map(|f| f.mode), Some(0o644));
    assert_eq!(release.exports.len(), 1);
}

type Mutate = fn(&mut Doc);
type Expect = fn(&ReleaseError) -> bool;

fn refused(name: &str, mutate: Mutate, expect: Expect) {
    let mut doc = Doc::good();
    mutate(&mut doc);
    match Release::parse(doc.render().as_bytes()) {
        Ok(_) => panic!("{name}: accepted"),
        Err(e) => assert!(expect(&e), "{name}: wrong refusal {e:?}"),
    }
}

#[test]
fn hostile_and_broken_releases_are_refused_for_the_right_reason() {
    let table: Vec<(&str, Mutate, Expect)> = vec![
        (
            "newer format",
            |d| d.format = "2".into(),
            |e| matches!(e, ReleaseError::UnsupportedFormat(2)),
        ),
        (
            "not semver",
            |d| d.version = "two".into(),
            |e| matches!(e, ReleaseError::BadVersion(_)),
        ),
        (
            "build metadata",
            |d| d.version = "0.2.0+abc".into(),
            |e| matches!(e, ReleaseError::BadVersion(_)),
        ),
        (
            "bad date",
            |d| d.date = "\"yesterday\"".into(),
            |e| matches!(e, ReleaseError::BadDate(_)),
        ),
        (
            "no files",
            |d| d.files.clear(),
            |e| matches!(e, ReleaseError::NoFiles),
        ),
        (
            "zero size",
            |d| d.bundle_size = "0".into(),
            |e| matches!(e, ReleaseError::BadBundle(_)),
        ),
        (
            "huge size",
            |d| d.bundle_size = "99999999999999".into(),
            |e| matches!(e, ReleaseError::BadBundle(_)),
        ),
        (
            "bundle name with a slash",
            |d| d.bundle_name = "../bundle".into(),
            |e| matches!(e, ReleaseError::BadBundle(_)),
        ),
        (
            "bundle name hidden",
            |d| d.bundle_name = ".bundle".into(),
            |e| matches!(e, ReleaseError::BadBundle(_)),
        ),
        (
            "short digest",
            |d| d.bundle_sha = "abc".into(),
            |e| matches!(e, ReleaseError::Unreadable(_)),
        ),
        (
            "upper-case digest",
            |d| d.bundle_sha = "A".repeat(64),
            |e| matches!(e, ReleaseError::Unreadable(_)),
        ),
        (
            "absolute path",
            |d| d.files.push(f("/etc/passwd", "x")),
            |e| matches!(e, ReleaseError::Unreadable(_)),
        ),
        (
            "dot-dot path",
            |d| d.files.push(f("bin/../../etc/x", "x")),
            |e| matches!(e, ReleaseError::Unreadable(_)),
        ),
        (
            "reserved folder",
            |d| d.files.push(f(".slipway/release.toml", "x")),
            |e| matches!(e, ReleaseError::Unreadable(_)),
        ),
        (
            "duplicate file",
            |d| d.files.push(f("bin/casement", "other")),
            |e| matches!(e, ReleaseError::DuplicateFile(_)),
        ),
        (
            "file inside file",
            |d| d.files.push(f("bin/casement/x", "x")),
            |e| matches!(e, ReleaseError::FileInsideFile(_)),
        ),
        (
            "group-writable mode",
            |d| d.files[0].mode = "0o775".into(),
            |e| matches!(e, ReleaseError::BadFile { .. }),
        ),
        (
            "setuid mode",
            |d| d.files[0].mode = "0o4755".into(),
            |e| matches!(e, ReleaseError::BadFile { .. }),
        ),
        (
            "unreadable mode",
            |d| d.files[0].mode = "0o000".into(),
            |e| matches!(e, ReleaseError::BadFile { .. }),
        ),
        (
            "link outside the allowed places",
            |d| d.exports[0].0 = "/etc/systemd/system/x.service".into(),
            |e| matches!(e, ReleaseError::BadExportLink(_)),
        ),
        (
            "link climbing out",
            |d| d.exports[0].0 = "/usr/share/applications/../../../etc/x".into(),
            |e| matches!(e, ReleaseError::BadExportLink(_)),
        ),
        (
            "link is a directory itself",
            |d| d.exports[0].0 = "/usr/local/bin".into(),
            |e| matches!(e, ReleaseError::BadExportLink(_)),
        ),
        (
            "link sibling of an allowed dir",
            |d| d.exports[0].0 = "/usr/local/bin2/x".into(),
            |e| matches!(e, ReleaseError::BadExportLink(_)),
        ),
        (
            "relative link",
            |d| d.exports[0].0 = "usr/local/bin/x".into(),
            |e| matches!(e, ReleaseError::BadExportLink(_)),
        ),
        (
            "duplicate link",
            |d| d.exports.push(d.exports[0].clone()),
            |e| matches!(e, ReleaseError::DuplicateExport(_)),
        ),
        (
            "export of something not shipped",
            |d| d.exports[0].1 = "bin/missing".into(),
            |e| matches!(e, ReleaseError::ExportTargetMissing(_)),
        ),
        (
            "export target climbs",
            |d| d.exports[0].1 = "../x".into(),
            |e| matches!(e, ReleaseError::Unreadable(_)),
        ),
        (
            "notes too long",
            |d| d.notes = "x".repeat(20_000),
            |e| matches!(e, ReleaseError::NotesTooLong),
        ),
    ];
    for (name, mutate, expect) in table {
        refused(name, mutate, expect);
    }
}

#[test]
fn garbage_input_is_refused_not_a_panic() {
    for bytes in [
        &b""[..],
        b"\xff\xfe",
        b"format = 1",
        b"[[file]]",
        &vec![b'a'; slipway_core::release::MAX_INDEX_BYTES + 1],
    ] {
        assert!(Release::parse(bytes).is_err());
    }
}

#[test]
fn an_export_must_point_at_a_listed_file_not_a_folder() {
    let mut doc = Doc::good();
    doc.exports[0].1 = "share/wayland-sessions".into();
    assert!(matches!(
        Release::parse(doc.render().as_bytes()),
        Err(ReleaseError::ExportTargetMissing(_))
    ));
}

#[test]
fn path_errors_carry_through() {
    let e = slipway_core::BundlePath::parse("a//b").unwrap_err();
    assert!(matches!(e, PathError::Climbing(_)));
    assert!(Sha256Hex::parse(&"0".repeat(64)).is_ok());
}
