//! Which old releases to delete.

use semver::Version;

/// Releases to delete: everything except the current one, the previous one and the newest
/// release that is newer than current (a published release that was never switched to).
pub fn prune(releases: &[Version], current: &Version, previous: Option<&Version>) -> Vec<Version> {
    let staged = releases
        .iter()
        .filter(|v| v.cmp_precedence(current).is_gt())
        .max_by(|a, b| a.cmp_precedence(b));
    let mut doomed: Vec<Version> = releases
        .iter()
        .filter(|v| *v != current && Some(*v) != previous && Some(*v) != staged)
        .cloned()
        .collect();
    doomed.sort();
    doomed.dedup();
    doomed
}
