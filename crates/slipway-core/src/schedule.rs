//! When the next daily check is due.

use serde::{Deserialize, Serialize};

/// Seconds since the Unix epoch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnixSeconds(pub i64);

/// Between two checks that went well.
pub const PERIOD_SECS: i64 = 24 * 60 * 60;
/// Added to the period at most, derived from a seed so each computer drifts differently.
pub const JITTER_SECS: u64 = 60 * 60;
/// Between a failed check and the retry.
pub const RETRY_SECS: i64 = 60 * 60;

/// How a check ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// It reached the release site and read an answer.
    Succeeded,
    /// It could not.
    Failed,
}

/// The last check that was tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attempt {
    /// When it ran.
    pub at: UnixSeconds,
    /// How it ended.
    pub outcome: Outcome,
}

/// When to check next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Due {
    /// Check now: never checked, overdue, or the clock was set back.
    Now,
    /// Check at this time.
    At(UnixSeconds),
}

/// The next check after `last`, seen from `now`. A good check is followed by one 24 hours plus a
/// jitter of up to an hour (from `seed`) later; a failed one by a retry an hour later. A start
/// that finds the check overdue checks at once.
pub fn next_check(last: Option<Attempt>, now: UnixSeconds, seed: u64) -> Due {
    let Some(last) = last else { return Due::Now };
    let jitter = i64::try_from(seed % JITTER_SECS).unwrap_or(0);
    let (wait, slack) = match last.outcome {
        Outcome::Succeeded => (PERIOD_SECS + jitter, PERIOD_SECS + JITTER_SECS as i64),
        Outcome::Failed => (RETRY_SECS, RETRY_SECS),
    };
    let due = last.at.0.saturating_add(wait);
    if due <= now.0 || due.saturating_sub(now.0) > slack {
        Due::Now
    } else {
        Due::At(UnixSeconds(due))
    }
}
