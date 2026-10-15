use crate::support::{Doc, f};
use slipway_core::{Audit, AuditError, EntryKind, Release, Sha256Hex};

fn release() -> Release {
    Release::parse(Doc::good().render().as_bytes()).expect("release")
}

fn digest(text: &str) -> Sha256Hex {
    Sha256Hex::of(text.as_bytes())
}

#[test]
fn a_complete_honest_bundle_passes() {
    let release = release();
    let mut audit = Audit::new(&release);
    audit.admit_dir("./").expect("root");
    audit.admit_dir("bin/").expect("bin");
    assert_eq!(
        audit
            .admit_file("./bin/casement", &digest("casement"))
            .expect("file")
            .mode,
        0o644
    );
    audit.admit_dir("share").expect("share");
    audit.admit_dir("share/wayland-sessions").expect("sessions");
    audit
        .admit_file(
            "share/wayland-sessions/quire.desktop",
            &digest("[Desktop Entry]"),
        )
        .expect("file");
    assert_eq!(audit.finish(), Ok(()));
}

#[test]
fn every_way_a_bundle_can_lie_is_caught() {
    let release = release();
    type Case = (
        &'static str,
        fn(&mut Audit<'_>) -> Result<(), AuditError>,
        fn(&AuditError) -> bool,
    );
    let table: Vec<Case> = vec![
        (
            "extra file",
            |a| a.admit_file("bin/extra", &digest("x")).map(drop),
            |e| matches!(e, AuditError::Unlisted(_)),
        ),
        (
            "changed file",
            |a| a.admit_file("bin/casement", &digest("evil")).map(drop),
            |e| matches!(e, AuditError::DigestMismatch(_)),
        ),
        (
            "file twice",
            |a| {
                a.admit_file("bin/casement", &digest("casement"))?;
                a.admit_file("bin/casement", &digest("casement")).map(drop)
            },
            |e| matches!(e, AuditError::Duplicate(_)),
        ),
        (
            "climbing path",
            |a| a.admit_file("../escape", &digest("x")).map(drop),
            |e| matches!(e, AuditError::BadPath(_)),
        ),
        (
            "absolute path",
            |a| a.admit_file("/etc/passwd", &digest("x")).map(drop),
            |e| matches!(e, AuditError::BadPath(_)),
        ),
        (
            "climbing after dot",
            |a| a.admit_file("./../escape", &digest("x")).map(drop),
            |e| matches!(e, AuditError::BadPath(_)),
        ),
        (
            "reserved folder",
            |a| {
                a.admit_file(".slipway/release.toml", &digest("x"))
                    .map(drop)
            },
            |e| matches!(e, AuditError::BadPath(_)),
        ),
        (
            "extra folder",
            |a| a.admit_dir("etc"),
            |e| matches!(e, AuditError::UnlistedDir(_)),
        ),
        (
            "folder where a file is listed",
            |a| a.admit_dir("bin/casement"),
            |e| matches!(e, AuditError::Mixed(_)),
        ),
        (
            "symlink",
            |a| Err(a.refuse("bin/link", EntryKind::Symlink)),
            |e| {
                matches!(
                    e,
                    AuditError::Forbidden {
                        kind: EntryKind::Symlink,
                        ..
                    }
                )
            },
        ),
        (
            "device",
            |a| Err(a.refuse("dev/null", EntryKind::Device)),
            |e| {
                matches!(
                    e,
                    AuditError::Forbidden {
                        kind: EntryKind::Device,
                        ..
                    }
                )
            },
        ),
        (
            "missing file",
            |a| {
                a.admit_file("bin/casement", &digest("casement"))?;
                a.admit_dir("share")?;
                Ok(())
            },
            |_| true,
        ),
    ];
    for (name, case, expect) in table {
        let mut audit = Audit::new(&release);
        let result = case(&mut audit).and_then(|()| audit.finish());
        match result {
            Ok(()) => panic!("{name}: accepted"),
            Err(e) => assert!(expect(&e), "{name}: {e:?}"),
        }
    }
}

#[test]
fn a_missing_file_is_named() {
    let release = release();
    let mut audit = Audit::new(&release);
    audit
        .admit_file("bin/casement", &digest("casement"))
        .expect("file");
    assert_eq!(
        audit.finish(),
        Err(AuditError::Missing(
            "share/wayland-sessions/quire.desktop".into()
        ))
    );
}

#[test]
fn listing_checks_before_anything_is_written() {
    let mut doc = Doc::good();
    doc.files.push(f("bin/other", "o"));
    let release = Release::parse(doc.render().as_bytes()).expect("release");
    let mut audit = Audit::new(&release);
    assert!(audit.listing("bin/other").is_ok());
    assert!(matches!(
        audit.listing("bin/nope"),
        Err(AuditError::Unlisted(_))
    ));
    audit.admit_file("bin/other", &digest("o")).expect("file");
    assert!(matches!(
        audit.listing("bin/other"),
        Err(AuditError::Duplicate(_))
    ));
}
