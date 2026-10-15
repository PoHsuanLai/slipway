use semver::Version;
use slipway_core::{Boot, LoginSession, Pending, PendingStage, Restore, decide_boot};

fn v(text: &str) -> Version {
    Version::parse(text).expect("version")
}

/// A login: `boot` and `session`, either may be unknown (empty text).
fn login(boot: &str, session: &str) -> LoginSession {
    let part = |t: &str| (!t.is_empty()).then(|| t.to_owned());
    LoginSession {
        boot: part(boot),
        session: part(session),
    }
}

/// A marker at `version`, `stage`, last set in `set_in` (None for a marker written before the
/// login was recorded).
fn pending(version: &str, stage: PendingStage, set_in: Option<LoginSession>) -> Option<Pending> {
    Some(Pending {
        version: v(version),
        stage,
        session: set_in,
    })
}

/// name, current, pending marker, confirmed, whether a previous release exists, the login this
/// start belongs to, the decision
type Row<'a> = (
    &'a str,
    Option<&'a str>,
    Option<Pending>,
    Option<&'a str>,
    Restore,
    LoginSession,
    Boot,
);

#[test]
fn the_start_up_decision_table() {
    use PendingStage::{AwaitingFirstLogin as First, FirstLoginSeen as Seen};
    use Restore::{Impossible, Possible};
    let table: Vec<Row> = vec![
        (
            "no marker",
            Some("0.2.0"),
            None,
            None,
            Possible,
            login("b1", "3"),
            Boot::Nothing,
        ),
        (
            "first start after install, in a later login",
            Some("0.2.0"),
            pending("0.2.0", First, Some(login("b1", "3"))),
            None,
            Possible,
            login("b1", "4"),
            Boot::MarkFirstLoginSeen,
        ),
        (
            "first start after install, after a reboot",
            Some("0.2.0"),
            pending("0.2.0", First, Some(login("b1", "3"))),
            None,
            Possible,
            login("b2", "3"),
            Boot::MarkFirstLoginSeen,
        ),
        (
            "daemon restarted in the session that installed",
            Some("0.2.0"),
            pending("0.2.0", First, Some(login("b1", "3"))),
            None,
            Possible,
            login("b1", "3"),
            Boot::KeepWaiting,
        ),
        (
            "marker from before the login was recorded, first start",
            Some("0.2.0"),
            pending("0.2.0", First, None),
            None,
            Possible,
            login("b1", "3"),
            Boot::MarkFirstLoginSeen,
        ),
        (
            "second login, never confirmed",
            Some("0.2.0"),
            pending("0.2.0", Seen, Some(login("b1", "4"))),
            None,
            Possible,
            login("b1", "5"),
            Boot::RollBack,
        ),
        (
            "second login, never confirmed, nothing to restore",
            Some("0.2.0"),
            pending("0.2.0", Seen, Some(login("b1", "4"))),
            None,
            Impossible,
            login("b1", "5"),
            Boot::GiveUp,
        ),
        (
            "daemon restarted in the first login, not confirmed yet",
            Some("0.2.0"),
            pending("0.2.0", Seen, Some(login("b1", "4"))),
            None,
            Possible,
            login("b1", "4"),
            Boot::KeepWaiting,
        ),
        (
            "daemon restarted in the first login, nothing to restore",
            Some("0.2.0"),
            pending("0.2.0", Seen, Some(login("b1", "4"))),
            None,
            Impossible,
            login("b1", "4"),
            Boot::KeepWaiting,
        ),
        (
            "same session number after a reboot is another login",
            Some("0.2.0"),
            pending("0.2.0", Seen, Some(login("b1", "4"))),
            None,
            Possible,
            login("b2", "4"),
            Boot::RollBack,
        ),
        (
            "no session numbers, same boot: cannot tell, so wait",
            Some("0.2.0"),
            pending("0.2.0", Seen, Some(login("b1", ""))),
            None,
            Possible,
            login("b1", ""),
            Boot::KeepWaiting,
        ),
        (
            "no session numbers, another boot",
            Some("0.2.0"),
            pending("0.2.0", Seen, Some(login("b1", ""))),
            None,
            Possible,
            login("b2", ""),
            Boot::RollBack,
        ),
        (
            "boot unknown: the same login is not provable",
            Some("0.2.0"),
            pending("0.2.0", Seen, Some(login("", "4"))),
            None,
            Possible,
            login("", "4"),
            Boot::RollBack,
        ),
        (
            "marker from before the login was recorded, second start",
            Some("0.2.0"),
            pending("0.2.0", Seen, None),
            None,
            Possible,
            login("b1", "4"),
            Boot::RollBack,
        ),
        (
            "confirmed in the first session",
            Some("0.2.0"),
            pending("0.2.0", Seen, Some(login("b1", "4"))),
            Some("0.2.0"),
            Possible,
            login("b1", "5"),
            Boot::Settle,
        ),
        (
            "confirmed before the first start",
            Some("0.2.0"),
            pending("0.2.0", First, Some(login("b1", "3"))),
            Some("0.2.0"),
            Possible,
            login("b1", "4"),
            Boot::Settle,
        ),
        (
            "confirmed, in the same login",
            Some("0.2.0"),
            pending("0.2.0", Seen, Some(login("b1", "4"))),
            Some("0.2.0"),
            Possible,
            login("b1", "4"),
            Boot::Settle,
        ),
        (
            "an older version was confirmed",
            Some("0.2.0"),
            pending("0.2.0", Seen, Some(login("b1", "4"))),
            Some("0.1.0"),
            Possible,
            login("b1", "5"),
            Boot::RollBack,
        ),
        (
            "marker for another version (someone switched by hand)",
            Some("0.1.0"),
            pending("0.2.0", Seen, Some(login("b1", "4"))),
            None,
            Possible,
            login("b1", "4"),
            Boot::Settle,
        ),
        (
            "nothing installed",
            None,
            pending("0.2.0", First, Some(login("b1", "3"))),
            None,
            Impossible,
            login("b1", "4"),
            Boot::Settle,
        ),
    ];
    for (name, current, marker, confirmed, restore, here, expected) in table {
        let current = current.map(v);
        let confirmed = confirmed.map(v);
        assert_eq!(
            decide_boot(
                current.as_ref(),
                marker.as_ref(),
                confirmed.as_ref(),
                restore,
                &here
            ),
            expected,
            "{name}"
        );
    }
}

#[test]
fn a_marker_carries_its_login_through_the_stages() {
    let installed = Pending::installed(v("0.2.0"), login("b1", "3"));
    assert_eq!(installed.stage, PendingStage::AwaitingFirstLogin);
    assert_eq!(installed.session, Some(login("b1", "3")));
    let seen = installed.first_login_seen(login("b1", "4"));
    assert_eq!(seen.stage, PendingStage::FirstLoginSeen);
    assert_eq!(seen.session, Some(login("b1", "4")));
    assert_eq!(seen.version, v("0.2.0"));
}
