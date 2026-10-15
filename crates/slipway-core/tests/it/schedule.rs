use slipway_core::{Attempt, Due, Outcome, UnixSeconds, next_check};

const T: i64 = 1_800_000_000;
const HOUR: i64 = 3600;
const DAY: i64 = 24 * HOUR;

fn at(secs: i64, outcome: Outcome) -> Option<Attempt> {
    Some(Attempt {
        at: UnixSeconds(secs),
        outcome,
    })
}

#[test]
fn the_daily_schedule_table() {
    let table = [
        ("never checked", None, T, 0, Due::Now),
        (
            "just checked, no jitter",
            at(T, Outcome::Succeeded),
            T,
            0,
            Due::At(UnixSeconds(T + DAY)),
        ),
        (
            "just checked, jitter 1000",
            at(T, Outcome::Succeeded),
            T,
            1000,
            Due::At(UnixSeconds(T + DAY + 1000)),
        ),
        (
            "jitter wraps at an hour",
            at(T, Outcome::Succeeded),
            T,
            3600 + 5,
            Due::At(UnixSeconds(T + DAY + 5)),
        ),
        (
            "overdue by a minute",
            at(T - DAY - 60, Outcome::Succeeded),
            T,
            0,
            Due::Now,
        ),
        (
            "due exactly now",
            at(T - DAY, Outcome::Succeeded),
            T,
            0,
            Due::Now,
        ),
        (
            "jitter not yet over",
            at(T - DAY, Outcome::Succeeded),
            T,
            600,
            Due::At(UnixSeconds(T + 600)),
        ),
        (
            "failed check retries in an hour",
            at(T, Outcome::Failed),
            T,
            1234,
            Due::At(UnixSeconds(T + HOUR)),
        ),
        (
            "failed check, hour passed",
            at(T - HOUR, Outcome::Failed),
            T,
            0,
            Due::Now,
        ),
        (
            "clock set back a week",
            at(T + 7 * DAY, Outcome::Succeeded),
            T,
            0,
            Due::Now,
        ),
        (
            "failed, clock set back",
            at(T + DAY, Outcome::Failed),
            T,
            0,
            Due::Now,
        ),
    ];
    for (name, last, now, seed, expected) in table {
        assert_eq!(next_check(last, UnixSeconds(now), seed), expected, "{name}");
    }
}
