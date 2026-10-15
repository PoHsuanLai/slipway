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

/// Whether a version found by a check is put in front of the person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offer {
    /// Newer than what runs and not refused: offer it.
    Present,
    /// A release that was rolled back (or an older one): not offered again.
    Refused,
    /// Not newer than what runs.
    NotNewer,
}

/// Decides whether `found` is offered, given the version installed and the newest version that was
/// rolled back. A refused version stays refused, and so does everything older than it, until a
/// newer one is published.
pub fn offer(installed: &Version, found: &Version, refused: Option<&Version>) -> Offer {
    if standing(installed, found) != Standing::Newer {
        return Offer::NotNewer;
    }
    match refused {
        Some(refused) if found.cmp_precedence(refused) != Ordering::Greater => Offer::Refused,
        _ => Offer::Present,
    }
}
