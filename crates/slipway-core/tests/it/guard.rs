use semver::Version;
use slipway_core::{Guard, GuardRecord, MAX_UNCONFIRMED_BOOTS, Restore, Trust};

fn v(text: &str) -> Version {
    Version::parse(text).expect("version")
}

/// Starts `version` in each boot in turn and returns the record and every verdict.
fn starts(
    from: GuardRecord,
    version: &str,
    boots: &[&str],
    restore: Restore,
) -> (GuardRecord, Vec<Guard>) {
    let mut record = from;
    let mut verdicts = Vec::new();
    for boot in boots {
        let (next, verdict) = record.started(&v(version), boot, restore);
        record = next;
        verdicts.push(verdict);
    }
    (record, verdicts)
}

#[test]
fn a_release_gets_two_boots_and_the_third_unconfirmed_one_rolls_back() {
    assert_eq!(MAX_UNCONFIRMED_BOOTS, 2);
    let (record, verdicts) = starts(
        GuardRecord::default(),
        "0.2.0",
        &["b1", "b2", "b3", "b4"],
        Restore::Possible,
    );
    assert_eq!(
        verdicts,
        vec![Guard::Fine, Guard::Fine, Guard::RollBack, Guard::RollBack]
    );
    assert_eq!(record.version, Some(v("0.2.0")));
    assert_eq!(record.trust, Trust::Unconfirmed);
}

#[test]
fn only_different_boots_count_so_a_process_of_the_person_cannot_force_it() {
    let (record, verdicts) = starts(
        GuardRecord::default(),
        "0.2.0",
        &["b1"; 50],
        Restore::Possible,
    );
    assert!(verdicts.iter().all(|g| *g == Guard::Fine));
    assert_eq!(record.boots, vec!["b1".to_owned()]);
}

#[test]
fn a_confirmed_release_is_never_rolled_back_and_nothing_is_recorded() {
    let (record, verdicts) = starts(
        GuardRecord::confirmed(v("0.2.0")),
        "0.2.0",
        &["b1", "b2", "b3", "b4"],
        Restore::Possible,
    );
    assert!(verdicts.iter().all(|g| *g == Guard::Fine));
    assert_eq!(record, GuardRecord::confirmed(v("0.2.0")));
}

#[test]
fn nothing_to_go_back_to_means_nothing_is_rolled_back() {
    let (_, verdicts) = starts(
        GuardRecord::default(),
        "0.2.0",
        &["b1", "b2", "b3", "b4"],
        Restore::Impossible,
    );
    assert!(verdicts.iter().all(|g| *g == Guard::Fine));
}

#[test]
fn a_record_about_another_release_starts_again() {
    let stale = GuardRecord {
        version: Some(v("0.1.0")),
        trust: Trust::Unconfirmed,
        boots: vec!["b1".into(), "b2".into(), "b3".into()],
    };
    let (record, verdict) = stale.started(&v("0.2.0"), "b4", Restore::Possible);
    assert_eq!(verdict, Guard::Fine);
    assert_eq!(record.version, Some(v("0.2.0")));
    assert_eq!(record.boots, vec!["b4".to_owned()]);
}

#[test]
fn the_record_keeps_only_the_latest_boots() {
    let names: Vec<String> = (0..20).map(|n| format!("b{n}")).collect();
    let boots: Vec<&str> = names.iter().map(String::as_str).collect();
    let (record, _) = starts(GuardRecord::default(), "0.2.0", &boots, Restore::Impossible);
    assert_eq!(record.boots.len(), 8);
    assert_eq!(record.boots.last().map(String::as_str), Some("b19"));
}

#[test]
fn the_record_round_trips_through_text() {
    let record = GuardRecord {
        version: Some(v("0.2.0")),
        trust: Trust::Unconfirmed,
        boots: vec!["b1".into()],
    };
    let text = toml::to_string(&record).expect("write");
    assert_eq!(toml::from_str::<GuardRecord>(&text).expect("read"), record);
    assert_eq!(
        toml::from_str::<GuardRecord>("").expect("empty"),
        GuardRecord::default()
    );
}
