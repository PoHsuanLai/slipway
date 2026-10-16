//! The three privileged programs' command lines.

use slipway_root::helper::{HealthRequest, Parsed, parse_health, parse_install, parse_rollback};
use std::path::PathBuf;

fn words(args: &[&str]) -> impl Iterator<Item = String> {
    args.iter()
        .map(|a| (*a).to_owned())
        .collect::<Vec<_>>()
        .into_iter()
}

#[test]
fn slipway_apply_installs_a_folder_and_nothing_else() {
    let table: [(&[&str], Parsed<PathBuf>); 8] = [
        (
            &["/var/tmp/stage"],
            Parsed::Run(PathBuf::from("/var/tmp/stage")),
        ),
        (&["--help"], Parsed::Help),
        (&["-h"], Parsed::Help),
        // Going back is not this program's business: it has its own, behind a password.
        (&["--rollback"], Parsed::Bad),
        (&["--other"], Parsed::Bad),
        (&[], Parsed::Bad),
        (&["a", "b"], Parsed::Bad),
        (&["--help", "x"], Parsed::Bad),
    ];
    for (args, expected) in table {
        assert_eq!(parse_install(words(args)), expected, "{args:?}");
    }
}

#[test]
fn slipway_rollback_takes_no_arguments() {
    let table: [(&[&str], Parsed<()>); 5] = [
        (&[], Parsed::Run(())),
        (&["--help"], Parsed::Help),
        (&["-h"], Parsed::Help),
        (&["/some/folder"], Parsed::Bad),
        (&["--help", "x"], Parsed::Bad),
    ];
    for (args, expected) in table {
        assert_eq!(parse_rollback(words(args)), expected, "{args:?}");
    }
}

#[test]
fn slipway_health_knows_two_words() {
    let table: [(&[&str], Parsed<HealthRequest>); 7] = [
        (&["recover"], Parsed::Run(HealthRequest::Recover)),
        (&["confirmed"], Parsed::Run(HealthRequest::Confirmed)),
        (&["--help"], Parsed::Help),
        (&["rollback"], Parsed::Bad),
        (&["recover", "now"], Parsed::Bad),
        (&[], Parsed::Bad),
        (&["--rollback"], Parsed::Bad),
    ];
    for (args, expected) in table {
        assert_eq!(parse_health(words(args)), expected, "{args:?}");
    }
}
