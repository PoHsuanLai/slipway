//! The service's side of a helper run: what it reads back, and what it does when the helper hangs.
//! A shell and scratch scripts stand in for `pkexec` and the helper; nothing privileged runs.

use slipway::elevate::{Elevate, Pkexec};
use slipway::report::Report;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A shell script that plays the helper, run through `sh` the way `pkexec` would run the real one.
fn helper(dir: &Path, script: &str, patience: Duration) -> Pkexec {
    let path = dir.join("helper.sh");
    std::fs::write(&path, script).expect("script");
    Pkexec::with_program(PathBuf::from("sh"), path, patience)
}

#[test]
fn what_the_helper_says_when_it_succeeds_reaches_the_service() {
    let dir = tempfile::tempdir().expect("tempdir");
    let script = "echo 'current 0.2.0'\n\
                  echo 'left-alone /etc/quire/intentd-callers.toml'\n\
                  echo 'warning the security labels could not be set'\n\
                  echo 'chatter on the error output' >&2\n\
                  exit 0\n";
    let report = helper(dir.path(), script, Duration::from_secs(20))
        .install(Path::new("/staged"))
        .expect("success");
    assert_eq!(report.version, Some(semver::Version::new(0, 2, 0)));
    assert_eq!(
        report.conflicts,
        vec!["/etc/quire/intentd-callers.toml".to_owned()]
    );
    assert_eq!(
        report.warnings,
        vec!["the security labels could not be set".to_owned()]
    );
}

#[test]
fn a_failing_helper_gives_its_exit_code_and_the_last_line_it_wrote() {
    let dir = tempfile::tempdir().expect("tempdir");
    let script = "echo 'first' >&2\necho 'the last line' >&2\necho >&2\nexit 4\n";
    let failure = helper(dir.path(), script, Duration::from_secs(20))
        .rollback()
        .expect_err("fails");
    assert_eq!(failure.code, Some(4));
    assert_eq!(failure.detail, "the last line");
}

#[test]
fn a_helper_that_never_finishes_is_given_up_on() {
    let dir = tempfile::tempdir().expect("tempdir");
    let started = Instant::now();
    let failure = helper(dir.path(), "exec sleep 60\n", Duration::from_millis(300))
        .install(Path::new("/staged"))
        .expect_err("gives up");
    assert_eq!(failure.code, None);
    assert!(failure.detail.contains("in time"), "{}", failure.detail);
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "the service stopped waiting"
    );
}

#[test]
fn a_helper_that_cannot_be_started_is_a_failure_not_a_hang() {
    let missing = Pkexec::with_program(
        PathBuf::from("/nonexistent/pkexec"),
        PathBuf::from("/nonexistent/helper"),
        Duration::from_secs(5),
    );
    let failure = missing.rollback().expect_err("cannot start");
    assert_eq!(failure.code, None);
}

#[test]
fn a_report_round_trips_and_a_message_cannot_add_lines() {
    let report = Report {
        version: Some(semver::Version::new(1, 2, 3)),
        conflicts: vec![
            "/etc/quire/a-callers.toml".into(),
            "/usr/local/bin/sill".into(),
        ],
        warnings: vec!["a\ninjected line\ncurrent 9.9.9".into()],
    };
    let text = report.render();
    assert_eq!(text.lines().count(), 4, "{text}");
    let back = Report::parse(&text);
    assert_eq!(back.version, report.version);
    assert_eq!(back.conflicts, report.conflicts);
    assert_eq!(back.warnings.len(), 1);
    assert_eq!(Report::parse("nothing we know\n"), Report::default());
}
