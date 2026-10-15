//! Comparing the running version with an offered one.

use semver::Version;
use std::cmp::Ordering;

/// Where an offered version stands against the installed one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    /// Newer: worth offering.
    Newer,
    /// The same version.
    Same,
    /// Older: never offered as an update, only reachable through a rollback.
    Older,
}

/// Compares `offered` with `installed` by semver precedence. Build metadata is ignored, so two
/// versions that differ only there are the same.
pub fn standing(installed: &Version, offered: &Version) -> Standing {
    match offered.cmp_precedence(installed) {
        Ordering::Greater => Standing::Newer,
        Ordering::Equal => Standing::Same,
        Ordering::Less => Standing::Older,
    }
}
