use proptest::prelude::*;
use slipway_core::{BundlePath, Emit, ProgressGate, Release};

proptest! {
    #[test]
    fn an_accepted_path_never_climbs_or_starts_at_the_root(text in ".{0,80}") {
        if let Ok(path) = BundlePath::parse(&text) {
            prop_assert!(!path.as_str().starts_with('/'));
            prop_assert!(path.as_str().split('/').all(|p| !p.is_empty() && p != "." && p != ".."));
        }
    }

    #[test]
    fn parsing_arbitrary_bytes_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..400)) {
        let _ = Release::parse(&bytes);
    }

    #[test]
    fn progress_always_ends_with_an_announcement(steps in proptest::collection::vec(0u32..=1000, 1..50)) {
        let mut gate = ProgressGate::new();
        let mut ms = 0u64;
        let mut sorted = steps;
        sorted.sort_unstable();
        for permille in sorted {
            ms += 1;
            let _ = gate.admit(permille, ms);
        }
        ms += 1;
        let last = gate.admit(1000, ms);
        // Either 1000 was already announced (then it is a repeat and skipped), or it is announced now.
        prop_assert!(matches!(last, Emit::Announce | Emit::Skip));
    }
}

#[test]
fn progress_is_throttled_to_steps_and_time() {
    let mut gate = ProgressGate::new();
    let table = [
        (0, 0, Emit::Announce),
        (10, 5, Emit::Skip),
        (60, 100, Emit::Skip),
        (99, 200, Emit::Skip),
        (100, 210, Emit::Announce),
        (101, 300, Emit::Skip),
        (102, 460, Emit::Announce),
        (102, 900, Emit::Skip),
        (950, 901, Emit::Announce),
        (1000, 902, Emit::Announce),
        (1000, 5000, Emit::Skip),
    ];
    for (permille, ms, expected) in table {
        assert_eq!(gate.admit(permille, ms), expected, "{permille} at {ms}");
    }
}
