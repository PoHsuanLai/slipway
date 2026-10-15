use semver::Version;
use slipway_core::{Installed, PlanError, Step, plan_install, plan_rollback, prune};

fn v(text: &str) -> Version {
    Version::parse(text).expect("version")
}

fn installed(current: Option<&str>, previous: Option<&str>, releases: &[&str]) -> Installed {
    Installed {
        current: current.map(v),
        previous: previous.map(v),
        releases: releases.iter().map(|r| v(r)).collect(),
    }
}

#[test]
fn installing_over_a_current_release_is_ordered_so_every_prefix_is_safe() {
    let steps = plan_install(
        &installed(Some("0.1.0"), Some("0.0.9"), &["0.0.9", "0.1.0"]),
        &v("0.2.0"),
    )
    .expect("plan");
    assert_eq!(
        steps,
        vec![
            Step::Unpack(v("0.2.0")),
            Step::Publish(v("0.2.0")),
            Step::SetPrevious(v("0.1.0")),
            Step::SwitchCurrent(v("0.2.0")),
            Step::SyncExports {
                from: Some(v("0.1.0")),
                to: v("0.2.0")
            },
            Step::Prune(vec![v("0.0.9")]),
        ]
    );
}

#[test]
fn a_first_install_has_no_previous() {
    let steps = plan_install(&Installed::default(), &v("0.1.0")).expect("plan");
    assert_eq!(
        steps,
        vec![
            Step::Unpack(v("0.1.0")),
            Step::Publish(v("0.1.0")),
            Step::SwitchCurrent(v("0.1.0")),
            Step::SyncExports {
                from: None,
                to: v("0.1.0")
            },
            Step::Prune(vec![]),
        ]
    );
}

#[test]
fn a_downgrade_or_a_repeat_is_never_planned() {
    let here = installed(Some("0.2.0"), None, &["0.2.0"]);
    for target in ["0.2.0", "0.1.0", "0.2.0-rc.1", "0.2.0+other"] {
        let err = plan_install(&here, &v(target)).expect_err(target);
        assert!(matches!(err, PlanError::NotNewer { .. }), "{target}");
    }
}

#[test]
fn rollback_swaps_current_and_previous() {
    let steps = plan_rollback(&installed(
        Some("0.2.0"),
        Some("0.1.0"),
        &["0.1.0", "0.2.0"],
    ))
    .expect("plan");
    assert_eq!(
        steps,
        vec![
            Step::SetPrevious(v("0.2.0")),
            Step::SwitchCurrent(v("0.1.0")),
            Step::SyncExports {
                from: Some(v("0.2.0")),
                to: v("0.1.0")
            },
            Step::Prune(vec![]),
        ]
    );
}

#[test]
fn rollback_refusals() {
    let table = [
        (installed(None, None, &[]), PlanError::NothingInstalled),
        (
            installed(Some("0.2.0"), None, &["0.2.0"]),
            PlanError::NoPrevious,
        ),
        (
            installed(Some("0.2.0"), Some("0.2.0"), &["0.2.0"]),
            PlanError::NoPrevious,
        ),
        (
            installed(Some("0.2.0"), Some("0.1.0"), &["0.2.0"]),
            PlanError::NoPrevious,
        ),
    ];
    for (state, expected) in table {
        assert_eq!(plan_rollback(&state), Err(expected), "{state:?}");
    }
}

#[test]
fn prune_keeps_current_previous_and_one_staged() {
    let all: Vec<Version> = ["0.0.1", "0.0.9", "0.1.0", "0.2.0", "0.3.0", "0.4.0"]
        .iter()
        .map(|s| v(s))
        .collect();
    let table = [
        // current 0.2.0, previous 0.1.0: 0.4.0 is the newest unswitched release and stays
        ("0.2.0", Some("0.1.0"), vec!["0.0.1", "0.0.9", "0.3.0"]),
        (
            "0.4.0",
            Some("0.3.0"),
            vec!["0.0.1", "0.0.9", "0.1.0", "0.2.0"],
        ),
        ("0.2.0", None, vec!["0.0.1", "0.0.9", "0.1.0", "0.3.0"]),
    ];
    for (current, previous, doomed) in table {
        let got = prune(&all, &v(current), previous.map(v).as_ref());
        let want: Vec<Version> = doomed.iter().map(|s| v(s)).collect();
        assert_eq!(got, want, "current {current}");
    }
}
