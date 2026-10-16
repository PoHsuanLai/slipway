use crate::support::{Accept, Recipe, Reject, Sandbox, build, build_around, pack_special, v};
use slipway_core::AuditError;
use slipway_root::apply::{ApplyError, ExitCode, StagedError, install};

/// After a refusal the system is as it was: the given release (if any) is current, and no unfinished
/// unpack or new release directory is left.
fn untouched(sandbox: &Sandbox, current: Option<&str>) {
    assert_eq!(sandbox.current(), current.map(v));
    let leftovers: Vec<String> = sandbox
        .releases()
        .into_iter()
        .filter(|r| Some(r.as_str()) != current)
        .collect();
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
}

#[test]
fn a_changed_file_is_refused_and_nothing_is_installed() {
    let sandbox = Sandbox::new();
    let mut recipe = Recipe::new("0.1.0");
    recipe.packed = Some(vec![
        ("bin/casement", "casement EVIL"),
        ("share/wayland-sessions/quire.desktop", "[Desktop Entry]"),
    ]);
    let err = sandbox.install(&build(&recipe)).expect_err("tampered");
    assert!(
        matches!(err, ApplyError::Bundle(AuditError::DigestMismatch(_))),
        "{err}"
    );
    assert_eq!(err.exit_code(), ExitCode::Unverified);
    untouched(&sandbox, None);
}

#[test]
fn a_bundle_with_an_extra_file_is_refused() {
    let sandbox = Sandbox::new();
    sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    let mut recipe = Recipe::new("0.2.0");
    let mut packed = recipe.listed.clone();
    packed.push(("bin/backdoor", "#!/bin/sh"));
    recipe.packed = Some(packed);
    let err = sandbox.install(&build(&recipe)).expect_err("extra");
    assert!(
        matches!(err, ApplyError::Bundle(AuditError::Unlisted(_))),
        "{err}"
    );
    untouched(&sandbox, Some("0.1.0"));
}

#[test]
fn a_bundle_missing_a_listed_file_is_refused() {
    let sandbox = Sandbox::new();
    let mut recipe = Recipe::new("0.1.0");
    recipe.packed = Some(vec![("bin/casement", "casement 1")]);
    let err = sandbox.install(&build(&recipe)).expect_err("missing");
    assert!(
        matches!(err, ApplyError::Bundle(AuditError::Missing(_))),
        "{err}"
    );
    untouched(&sandbox, None);
}

#[test]
fn links_and_devices_in_the_bundle_are_refused() {
    for kind in [
        tar::EntryType::Symlink,
        tar::EntryType::Link,
        tar::EntryType::Char,
        tar::EntryType::Fifo,
    ] {
        let sandbox = Sandbox::new();
        let recipe = Recipe::new("0.1.0");
        let bundle = pack_special(&recipe.listed, "bin/sneaky", kind);
        let err = sandbox
            .install(&build_around(&recipe, bundle))
            .expect_err("special");
        assert!(
            matches!(err, ApplyError::Bundle(AuditError::Forbidden { .. })),
            "{kind:?}: {err}"
        );
        untouched(&sandbox, None);
    }
}

#[test]
fn an_interrupted_unpack_leaves_the_current_release_alone() {
    let sandbox = Sandbox::new();
    sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    let recipe = Recipe::new("0.2.0");
    let whole = build(&recipe).bundle;
    // The release describes (and the digest covers) a bundle cut off half way, as a dropped download
    // that was somehow signed for would be: unpacking fails part way through.
    let cut = whole[..whole.len() / 2].to_vec();
    let err = sandbox
        .install(&build_around(&recipe, cut))
        .expect_err("truncated");
    assert!(
        matches!(err, ApplyError::Unpack(_) | ApplyError::Bundle(_)),
        "{err}"
    );
    untouched(&sandbox, Some("0.1.0"));
    assert_eq!(
        std::fs::read_link(sandbox.layout.current()).expect("link"),
        std::path::PathBuf::from("releases/0.1.0")
    );
}

#[test]
fn a_bundle_that_is_not_the_signed_one_is_refused_before_anything_is_unpacked() {
    let sandbox = Sandbox::new();
    let built = build(&Recipe::new("0.1.0"));
    let dir = sandbox.stage(&built);
    let mut bytes = built.bundle.clone();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(dir.join("bundle.tar.zst"), &bytes).expect("swap bundle");
    let err = install(&sandbox.layout, &dir, &Accept).expect_err("swapped");
    assert!(
        matches!(err, ApplyError::Staged(StagedError::BundleDigest)),
        "{err}"
    );
    untouched(&sandbox, None);

    // A bundle of the right digest but the wrong length cannot happen without breaking the digest, so
    // the size check is exercised by lying about the size instead.
    let mut lying = built;
    lying.release_toml = lying
        .release_toml
        .replace(&format!("size = {}", lying.bundle.len()), "size = 5");
    let dir = sandbox.stage(&lying);
    let err = install(&sandbox.layout, &dir, &Accept).expect_err("size");
    assert!(
        matches!(err, ApplyError::Staged(StagedError::BundleSize { .. })),
        "{err}"
    );
}

#[test]
fn an_unsigned_or_wrongly_signed_release_is_refused() {
    let sandbox = Sandbox::new();
    let dir = sandbox.stage(&build(&Recipe::new("0.1.0")));
    let err = install(&sandbox.layout, &dir, &Reject).expect_err("unsigned");
    assert!(
        matches!(err, ApplyError::Staged(StagedError::Signature(_))),
        "{err}"
    );
    assert_eq!(err.exit_code(), ExitCode::Unverified);
    untouched(&sandbox, None);
}

#[test]
fn a_description_that_breaks_the_rules_is_refused_even_when_signed() {
    let sandbox = Sandbox::new();
    let mut recipe = Recipe::new("0.1.0");
    recipe.exports = vec![("/etc/systemd/system/evil.service", "bin/casement")];
    let err = sandbox
        .install(&build(&recipe))
        .expect_err("export outside the allowed places");
    assert!(
        matches!(err, ApplyError::Staged(StagedError::Release(_))),
        "{err}"
    );
    untouched(&sandbox, None);
}
