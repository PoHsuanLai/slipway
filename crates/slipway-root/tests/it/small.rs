//! Small pieces the privileged programs and the service share: the two system settings, the key, the
//! file operations and the reading of what is installed.

use crate::support::{Recipe, Sandbox, build, v};
use slipway_root::fsutil::{switch_link, write_atomic};
use slipway_root::keys::{KeyError, from_compiled, load_key};
use slipway_root::settings::{DEFAULT_PUBKEY_PATH, DEFAULT_ROOT, RootSettings, SettingsError};
use std::path::{Path, PathBuf};

#[test]
fn the_settings_give_root_and_key_and_ignore_the_rest() {
    let defaults = RootSettings::default();
    assert_eq!(defaults.root, PathBuf::from(DEFAULT_ROOT));
    assert_eq!(defaults.pubkey_path, PathBuf::from(DEFAULT_PUBKEY_PATH));
    let both = RootSettings {
        root: PathBuf::from("/srv/q"),
        pubkey_path: PathBuf::from("/srv/q.pub"),
    };
    let table = [
        ("", defaults.clone()),
        ("# only a comment\n", defaults.clone()),
        (
            "root = \"/srv/q\"\npubkey_path = \"/srv/q.pub\"\n",
            both.clone(),
        ),
        (
            "root = \"/srv/q\"\n",
            RootSettings {
                root: PathBuf::from("/srv/q"),
                ..defaults.clone()
            },
        ),
        (
            "pubkey_path = \"/srv/q.pub\"\n",
            RootSettings {
                pubkey_path: PathBuf::from("/srv/q.pub"),
                ..defaults.clone()
            },
        ),
        // The service's own settings are not this crate's business.
        (
            "root = \"/srv/q\"\npubkey_path = \"/srv/q.pub\"\nname = \"Lab\"\ncallers = [\"a.service\"]\n\
             [extra]\nroot = \"/not/this\"\n",
            both,
        ),
    ];
    for (text, expected) in table {
        let got = RootSettings::parse(text, Path::new("config.toml")).expect(text);
        assert_eq!(got, expected, "{text:?}");
    }
}

#[test]
fn damaged_or_mistyped_settings_are_refused_and_a_missing_file_is_all_defaults() {
    let table = [
        ("root = [", "parse"),
        ("root = 3\n", "not a path"),
        ("pubkey_path = true\n", "not a path"),
        ("root = [\"/a\"]\n", "not a path"),
    ];
    for (text, kind) in table {
        let err = RootSettings::parse(text, Path::new("config.toml")).expect_err(text);
        let matches = match kind {
            "parse" => matches!(err, SettingsError::Parse { .. }),
            _ => matches!(err, SettingsError::NotAPath { .. }),
        };
        assert!(matches, "{text:?}: {err}");
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = RootSettings::load(&dir.path().join("none.toml")).expect("missing is fine");
    assert_eq!(missing, RootSettings::default());
    let file = dir.path().join("config.toml");
    std::fs::write(&file, "root = \"/srv/q\"\n").expect("write");
    assert_eq!(
        RootSettings::load(&file).expect("load").root,
        PathBuf::from("/srv/q")
    );
    // A folder where the file should be is a read problem, not a silent default.
    assert!(matches!(
        RootSettings::load(dir.path()),
        Err(SettingsError::Read { .. })
    ));
}

#[test]
fn keys_load_from_a_file_or_the_compiled_in_text_and_say_why_not() {
    const KEY: &str = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("release.pub");
    std::fs::write(
        &file,
        format!("untrusted comment: minisign public key\n{KEY}\n"),
    )
    .expect("write");
    assert!(load_key(&file).is_ok());
    std::fs::write(&file, "garbage").expect("write");
    assert!(matches!(load_key(&file), Err(KeyError::Unreadable)));
    assert!(matches!(from_compiled(None), Err(KeyError::Missing)));
    assert!(matches!(from_compiled(Some("  ")), Err(KeyError::Missing)));
    assert!(matches!(
        from_compiled(Some("nonsense")),
        Err(KeyError::Unreadable)
    ));
    assert!(from_compiled(Some(KEY)).is_ok());
}

#[test]
fn links_are_switched_whole_and_files_replaced_whole() {
    let dir = tempfile::tempdir().expect("tempdir");
    let link = dir.path().join("current");
    switch_link(&link, std::path::Path::new("releases/1")).expect("first");
    switch_link(&link, std::path::Path::new("releases/2")).expect("second");
    assert_eq!(
        std::fs::read_link(&link).expect("link"),
        PathBuf::from("releases/2")
    );
    assert!(!dir.path().join("current.new").exists());

    let file = dir.path().join("a/b.txt");
    write_atomic(&file, b"one").expect("write");
    write_atomic(&file, b"two").expect("replace");
    assert_eq!(std::fs::read_to_string(&file).expect("read"), "two");
    assert!(!dir.path().join("a/b.txt.new").exists());
}

#[test]
fn installed_ignores_what_is_not_a_release() {
    let sandbox = Sandbox::new();
    sandbox
        .install(&build(&Recipe::new("0.1.0")))
        .expect("install");
    let releases = sandbox.layout.releases_dir();
    std::fs::create_dir(releases.join("not-a-version")).expect("dir");
    std::fs::create_dir(releases.join("0.2.0.partial")).expect("dir");
    std::fs::write(releases.join("0.3.0"), "a file, not a release").expect("file");
    let installed = sandbox.layout.installed().expect("installed");
    assert_eq!(installed.releases, vec![v("0.1.0")]);
    assert_eq!(installed.current, Some(v("0.1.0")));
}
