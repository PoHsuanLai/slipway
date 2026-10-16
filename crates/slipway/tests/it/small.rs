use crate::support::v;
use slipway::cli::{Command, parse};
use slipway::config::{Config, ConfigError};
use slipway::dirs::Dirs;
use slipway::elevate::ApplyFailure;
use slipway::login;
use slipway::problem::Problem;
use slipway::service::{InstallError, MAX_VERSION_BYTES, Wanted, is_from_owner};
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
fn the_system_file_decides_where_and_what_and_the_person_only_what_is_shown() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (system, user) = (dir.path().join("system.toml"), dir.path().join("user.toml"));
    std::fs::write(
        &system,
        "root = \"/srv/q\"\npubkey_path = \"/srv/q.pub\"\nname = \"Quire Lab\"\n",
    )
    .expect("write");
    // The person's file tries every setting; only the two display ones count.
    std::fs::write(
        &user,
        "root = \"/home/p/evil\"\npubkey_path = \"/home/p/evil.pub\"\nindex_url = \"https://evil.test\"\n\
         apply_path = \"/home/p/evil\"\nrollback_path = \"/home/p/evil\"\nhealth_path = \"/home/p/evil\"\n\
         settings_command = \"open-it now\"\n",
    )
    .expect("write");
    let config = Config::load_layered(&system, &user).expect("layered");
    assert_eq!(config.root, PathBuf::from("/srv/q"));
    assert_eq!(config.pubkey_path, PathBuf::from("/srv/q.pub"));
    assert_eq!(config.index_url, Config::default().index_url);
    assert_eq!(config.apply_path, Config::default().apply_path);
    assert_eq!(config.rollback_path, Config::default().rollback_path);
    assert_eq!(config.health_path, Config::default().health_path);
    assert_eq!(config.settings_command, "open-it now");
    assert_eq!(
        config.name, "Quire Lab",
        "the system's name stands unless the person names it"
    );

    std::fs::write(&user, "name = \"My Quire\"\nsettings_command = \"  \"\n").expect("write");
    let config = Config::load_layered(&system, &user).expect("layered");
    assert_eq!(config.name, "My Quire");
    assert_eq!(
        config.settings_command,
        Config::default().settings_command,
        "a blank command is ignored"
    );

    // Either file missing is fine; a damaged one is said so.
    let nowhere = dir.path().join("none.toml");
    assert_eq!(
        Config::load_layered(&nowhere, &nowhere).expect("defaults"),
        Config::default()
    );
    std::fs::write(&user, "name = [").expect("write");
    assert!(matches!(
        Config::load_layered(&system, &user),
        Err(ConfigError::Parse { .. })
    ));
}

#[test]
fn the_login_is_read_through_seams_and_unknown_parts_stay_unknown() {
    let env = |session: Option<&'static str>| {
        move |name: &str| {
            (name == "XDG_SESSION_ID")
                .then_some(session)
                .flatten()
                .map(OsString::from)
        }
    };
    let boot = |text: &'static str| {
        move |path: &std::path::Path| {
            assert_eq!(
                path,
                std::path::Path::new("/proc/sys/kernel/random/boot_id")
            );
            Ok(text.to_owned())
        }
    };
    let both = login::here(env(Some("c2")), boot("1f0e-aa\n"));
    assert_eq!(both.boot.as_deref(), Some("1f0e-aa"));
    assert_eq!(both.session.as_deref(), Some("c2"));

    let blank = login::here(env(Some("  ")), boot("\n"));
    assert_eq!((blank.boot, blank.session), (None, None));

    let unreadable = login::here(env(None), |_| {
        Err(std::io::Error::from(std::io::ErrorKind::NotFound))
    });
    assert_eq!((unreadable.boot, unreadable.session), (None, None));
}

#[test]
fn only_the_notification_servers_own_signal_counts() {
    let table = [
        (Some(":1.20"), Some(":1.20"), true),
        (Some(":1.99"), Some(":1.20"), false),
        (None, Some(":1.20"), false),
        (Some(":1.20"), None, false),
        (None, None, false),
    ];
    for (sender, owner, expected) in table {
        assert_eq!(
            is_from_owner(sender, owner),
            expected,
            "{sender:?} {owner:?}"
        );
    }
}

#[test]
fn the_newest_refused_version_is_the_one_kept() {
    let mut state = slipway::state::UserState::default();
    state.refuse(v("0.2.0"));
    state.refuse(v("0.1.0"));
    assert_eq!(state.refused, Some(v("0.2.0")));
    state.refuse(v("0.3.0"));
    assert_eq!(state.refused, Some(v("0.3.0")));

    let dir = tempfile::tempdir().expect("tempdir");
    let store = StateStore::new(dir.path().join("state.toml"));
    store.update(|s| s.refuse(v("0.3.0"))).expect("update");
    assert_eq!(store.load().refused, Some(v("0.3.0")));
}

#[test]
fn a_marker_from_before_the_login_was_recorded_still_reads() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("state.toml");
    std::fs::write(
        &path,
        "[pending]\nversion = \"0.2.0\"\nstage = \"first_login_seen\"\n",
    )
    .expect("write");
    let store = StateStore::new(path);
    let marker = store.load().pending.expect("the marker is read");
    assert_eq!(marker.session, None);
    // And a marker with its login survives a round trip through the file.
    let with_login = slipway_core::Pending::installed(
        v("0.3.0"),
        slipway_core::LoginSession {
            boot: Some("b1".into()),
            session: Some("4".into()),
        },
    );
    store
        .update(|s| s.pending = Some(with_login.clone()))
        .expect("update");
    assert_eq!(store.load().pending, Some(with_login));
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
        (vec!["recover"], Ok(Command::Recover)),
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
        Problem::NotAVersion,
        Problem::VersionChanged,
        Problem::InstallNotFromHere,
        Problem::RollbackNotFromHere,
        Problem::CallerUnknown,
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
fn only_a_real_version_of_sane_length_can_be_named_for_an_install() {
    let long = format!("1.0.0-{}", "a".repeat(MAX_VERSION_BYTES));
    let table = [
        ("0.2.0", true),
        ("10.20.30", true),
        ("1.2.3-rc.1", true),
        ("", false),
        ("0.2", false),
        ("v0.2.0", false),
        (" 0.2.0", false),
        ("0.2.0\n", false),
        ("latest", false),
        ("1.2.3.4", false),
        ("99999999999999999999.0.0", false),
        (long.as_str(), false),
    ];
    for (text, ok) in table {
        assert_eq!(Wanted::parse(text).is_ok(), ok, "{text:?}");
    }
    assert_eq!(Wanted::parse("nope"), Err(Problem::NotAVersion));
    let shown = Wanted::parse("0.2.0").expect("version");
    assert_eq!(shown.accepts(&v("0.2.0")), Ok(()));
    for other in ["0.2.1", "0.3.0", "0.1.0", "0.2.0-rc.1"] {
        assert_eq!(
            shown.accepts(&v(other)),
            Err(Problem::VersionChanged),
            "{other}"
        );
    }
}

#[test]
fn a_refused_install_has_a_name_of_its_own_and_a_plain_sentence() {
    use zbus::DBusError;
    let table = [
        (Problem::NotAVersion, "org.quire.Updates1.Error.NotAVersion"),
        (
            Problem::VersionChanged,
            "org.quire.Updates1.Error.VersionChanged",
        ),
        (
            Problem::InstallNotFromHere,
            "org.quire.Updates1.Error.AccessDenied",
        ),
        (
            Problem::CallerUnknown,
            "org.quire.Updates1.Error.AccessDenied",
        ),
        (Problem::Busy, "org.quire.Updates1.Error.Refused"),
        (Problem::Offline, "org.quire.Updates1.Error.Refused"),
    ];
    for (problem, name) in table {
        let error = InstallError::from(problem);
        assert_eq!(error.name().as_str(), name);
        assert_eq!(error.description(), Some(problem.plain()));
    }
}
