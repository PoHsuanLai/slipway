//! The version a caller of `Install(version)` says it showed the person.

use crate::problem::Problem;
use semver::Version;

/// The longest version text taken from a caller: far past any real version, short enough that a
/// peer cannot make the service parse something huge.
pub const MAX_VERSION_BYTES: usize = 64;

/// The version the person was shown. An install goes ahead only when it is the one that would be
/// installed, so an update that arrived between showing and pressing is never installed unseen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wanted(Version);

impl Wanted {
    /// The version named by `text`, or [`Problem::NotAVersion`] when it is too long or not a version.
    pub fn parse(text: &str) -> Result<Self, Problem> {
        if text.len() > MAX_VERSION_BYTES {
            return Err(Problem::NotAVersion);
        }
        Version::parse(text)
            .map(Self)
            .map_err(|_| Problem::NotAVersion)
    }

    /// Whether `offered` is the version that was shown; [`Problem::VersionChanged`] when it is not.
    pub fn accepts(&self, offered: &Version) -> Result<(), Problem> {
        if self.0 == *offered {
            Ok(())
        } else {
            Err(Problem::VersionChanged)
        }
    }
}
