use crate::support::{Recipe, Sandbox, build, v};
use slipway::cli::{Command, parse};
use slipway::config::{Config, ConfigError};
use slipway::dirs::Dirs;
use slipway::elevate::ApplyFailure;
use slipway::fsutil::{switch_link, write_atomic};
use slipway::keys::{KeyError, from_compiled, load_key};
use slipway::problem::Problem;
use slipway::state::StateStore;
use slipway_core::{Attempt, Outcome, UnixSeconds};
use std::ffi::OsString;
use std::path::PathBuf;

#[test]
fn the_configuration_has_defaults_and_overrides() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = Config::load(&dir.path().join("none.toml")).expect("missing is fine");
    assert_eq!(missing, Config::default());
    assert_eq!(missing.root, PathBuf::from("/opt/quire"));
    assert_eq!(missing.settings_command, "detent --page software-update");
    assert_eq!(missing.name, "Quire");

    let file = dir.path().join("config.toml");
    std::fs::write(&file, "root = \"/srv/q\"\nname = \"Quire Beta\"\n").expect("write");
    let config = Config::load(&file).expect("load");
    assert_eq!(config.root, PathBuf::from("/srv/q"));
    assert_eq!(config.name, "Quire Beta");
    assert_eq!(config.index_url, Config::default().index_url);

    std::fs::write(&file, "root = [").expect("write");
    assert!(matches!(
        Config::load(&file),
        Err(ConfigError::Parse { .. })
    ));
}

#[test]
fn directories_come_from_the_xdg_variables_then_home() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(v))
        }
    };
    let from_home = Dirs::resolve(env(&[("HOME", "/home/p")])).expect("dirs");
    assert_eq!(
        from_home.config_file,
        PathBuf::from("/home/p/.config/slipway/config.toml")
    );
    assert_eq!(
        from_home.state_file,
        PathBuf::from("/home/p/.local/state/slipway/state.toml")
    );
    assert_eq!(from_home.cache_dir, PathBuf::from("/home/p/.cache/slipway"));

    let set = Dirs::resolve(env(&[
        ("HOME", "/home/p"),
        ("XDG_CONFIG_HOME", "/c"),
        ("XDG_STATE_HOME", "/s"),
        ("XDG_CACHE_HOME", "/k"),
    ]))
    .expect("dirs");
    assert_eq!(set.config_file, PathBuf::from("/c/slipway/config.toml"));
    assert_eq!(set.state_file, PathBuf::from("/s/slipway/state.toml"));
    assert_eq!(set.cache_dir, PathBuf::from("/k/slipway"));

    // A relative XDG value is not a place; fall back to home.
    let relative =
        Dirs::resolve(env(&[("HOME", "/home/p"), ("XDG_CACHE_HOME", "cache")])).expect("dirs");
    assert_eq!(relative.cache_dir, PathBuf::from("/home/p/.cache/slipway"));
    assert!(Dirs::resolve(env(&[])).is_none());
}

#[test]
fn command_line_table() {
    let table = [
        (vec!["run"], Ok(Command::Run)),
        (vec!["check"], Ok(Command::Check)),
        (vec!["status"], Ok(Command::Status)),
        (vec!["install"], Ok(Command::Install)),
        (vec!["rollback"], Ok(Command::Rollback)),
        (vec!["confirm"], Ok(Command::Confirm)),
        (vec!["--help"], Ok(Command::Help)),
        (vec![], Err(())),
        (vec!["upgrade"], Err(())),
        (vec!["check", "now"], Err(())),
    ];
    for (args, expected) in table {
        let got = parse(args.iter().map(|a| (*a).to_owned())).map_err(|_| ());
        assert_eq!(got, expected, "{args:?}");
    }
}

#[test]
fn the_state_file_round_trips_and_is_replaced_whole() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = StateStore::new(dir.path().join("deep/state.toml"));
    assert_eq!(store.load().last_attempt(), None);
    store
        .update(|s| {
            s.last_check = Some(UnixSeconds(1_800_000_000));
            s.last_outcome = Some(Outcome::Failed);
            s.notified = Some(v("0.2.0"));
        })
        .expect("update");
    let again = StateStore::new(dir.path().join("deep/state.toml")).load();
    assert_eq!(
        again.last_attempt(),
        Some(Attempt {
            at: UnixSeconds(1_800_000_000),
            outcome: Outcome::Failed
        })
    );
    assert_eq!(again.notified, Some(v("0.2.0")));
    assert!(!dir.path().join("deep/state.toml.new").exists());

    std::fs::write(dir.path().join("deep/state.toml"), "this is not toml [").expect("garble");
    assert_eq!(
        store.load(),
        Default::default(),
        "a damaged memory is an empty one"
    );
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
fn what_a_person_reads_names_no_tool_or_protocol() {
    let failure = |code| ApplyFailure {
        code,
        detail: String::new(),
    };
    let table = [
        (Some(126), Problem::NotAllowed),
        (Some(127), Problem::NotAllowed),
        (Some(3), Problem::NothingToInstall),
        (Some(4), Problem::Unverified),
        (Some(5), Problem::Busy),
        (Some(6), Problem::NoKey),
        (Some(1), Problem::Other),
        (None, Problem::Other),
    ];
    for (code, expected) in table {
        assert_eq!(Problem::from_apply(&failure(code)), expected);
    }
    let all = [
        Problem::NoKey,
        Problem::KeyUnreadable,
        Problem::Offline,
        Problem::Unverified,
        Problem::NotAllowed,
        Problem::Busy,
        Problem::NothingToInstall,
        Problem::NothingToRestore,
        Problem::Other,
    ];
    for problem in all {
        let text = problem.plain().to_lowercase();
        for word in [
            "polkit",
            "pkexec",
            "dbus",
            "d-bus",
            "minisign",
            "sha",
            "signature",
            "tar",
            "zstd",
            "http",
            "exit code",
        ] {
            assert!(!text.contains(word), "{text:?} mentions {word}");
        }
    }
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
