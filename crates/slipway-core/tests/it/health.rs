use semver::Version;
use slipway_core::{Boot, Pending, PendingStage, Restore, decide_boot};

fn v(text: &str) -> Version {
    Version::parse(text).expect("version")
}

fn pending(version: &str, stage: PendingStage) -> Option<Pending> {
    Some(Pending {
        version: v(version),
        stage,
    })
}

#[test]
fn the_start_up_decision_table() {
    use PendingStage::{AwaitingFirstLogin as First, FirstLoginSeen as Seen};
    use Restore::{Impossible, Possible};
    let table: Vec<(
        &str,
        Option<&str>,
        Option<Pending>,
        Option<&str>,
        Restore,
        Boot,
    )> = vec![
        (
            "no marker",
            Some("0.2.0"),
            None,
            None,
            Possible,
            Boot::Nothing,
        ),
        (
            "first start after install",
            Some("0.2.0"),
            pending("0.2.0", First),
            None,
            Possible,
            Boot::MarkFirstLoginSeen,
        ),
        (
            "second start, never confirmed",
            Some("0.2.0"),
            pending("0.2.0", Seen),
            None,
            Possible,
            Boot::RollBack,
        ),
        (
            "second start, never confirmed, nothing to restore",
            Some("0.2.0"),
            pending("0.2.0", Seen),
            None,
            Impossible,
            Boot::GiveUp,
        ),
        (
            "confirmed in the first session",
            Some("0.2.0"),
            pending("0.2.0", Seen),
            Some("0.2.0"),
            Possible,
            Boot::Settle,
        ),
        (
            "confirmed before the first start",
            Some("0.2.0"),
            pending("0.2.0", First),
            Some("0.2.0"),
            Possible,
            Boot::Settle,
        ),
        (
            "an older version was confirmed",
            Some("0.2.0"),
            pending("0.2.0", Seen),
            Some("0.1.0"),
            Possible,
            Boot::RollBack,
        ),
        (
            "marker for another version (someone switched by hand)",
            Some("0.1.0"),
            pending("0.2.0", Seen),
            None,
            Possible,
            Boot::Settle,
        ),
        (
            "nothing installed",
            None,
            pending("0.2.0", First),
            None,
            Impossible,
            Boot::Settle,
        ),
    ];
    for (name, current, marker, confirmed, restore, expected) in table {
        let current = current.map(v);
        let confirmed = confirmed.map(v);
        assert_eq!(
            decide_boot(
                current.as_ref(),
                marker.as_ref(),
                confirmed.as_ref(),
                restore
            ),
            expected,
            "{name}"
        );
    }
}
