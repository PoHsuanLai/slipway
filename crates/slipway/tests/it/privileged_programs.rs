//! Who may ask the service to go back.

use slipway::config::Config;
use slipway::service::caller::{is_allowed, leaf_of};

#[test]
fn the_cgroup_names_the_unit_or_scope() {
    let table = [
        (
            "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-org.quire.Settings-1234.scope\n",
            Some("app-org.quire.Settings-1234.scope"),
        ),
        (
            "12:cpu:/x\n0::/user.slice/user@1000.service/session.slice/sill.service\n",
            Some("sill.service"),
        ),
        ("0::/user.slice/x.service/\n", Some("x.service")),
        ("12:cpu:/x\n", None),
        ("0::/\n", None),
        ("", None),
    ];
    for (text, expected) in table {
        assert_eq!(leaf_of(text), expected, "{text:?}");
    }
}

#[test]
fn only_the_shell_and_settings_may_ask_to_go_back() {
    let allowed = Config::default().callers;
    assert_eq!(allowed, vec!["sill.service", "org.quire.Settings"]);
    let table = [
        ("sill.service", true),
        ("app-org.quire.Settings-1234.scope", true),
        ("app-gnome-org.quire.Settings-99.scope", true),
        // Near misses.
        ("app-org.quire.Settings.scope", false),
        ("app-org.quire.Settings-x.scope", false),
        ("app-org.quire.SettingsEvil-1.scope", false),
        ("app-evilorg.quire.Settings-1.scope", false),
        ("evil-sill.service", false),
        ("sill.service.d", false),
        ("org.quire.Settings-1234.scope", false),
        // Everything else a person runs.
        ("app-kitty-77.scope", false),
        ("session-3.scope", false),
        ("dbus.service", false),
        ("slipway.service", false),
    ];
    for (leaf, expected) in table {
        assert_eq!(is_allowed(leaf, &allowed), expected, "{leaf}");
    }
    assert!(
        !is_allowed("sill.service", &[]),
        "an empty list lets nobody"
    );
}
