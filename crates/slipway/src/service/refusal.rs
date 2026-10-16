//! `org.quire.Updates1.Error.<Variant>`: why `Install(version)` was refused, as a caller sees it.

use crate::problem::Problem;

/// The errors `Install(version)` returns. Each carries the plain sentence for the person.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.quire.Updates1.Error")]
pub enum InstallError {
    /// The bus's own failures.
    #[zbus(error)]
    ZBus(zbus::Error),
    /// The text given is not a version (or is too long to be one).
    NotAVersion(String),
    /// The update on offer is not the version the caller showed: a newer one arrived, or none is on offer.
    VersionChanged(String),
    /// Anything else that stops the install from starting, such as work already in progress.
    Refused(String),
}

impl From<Problem> for InstallError {
    fn from(problem: Problem) -> Self {
        let text = problem.plain().to_owned();
        match problem {
            Problem::NotAVersion => Self::NotAVersion(text),
            Problem::VersionChanged => Self::VersionChanged(text),
            _ => Self::Refused(text),
        }
    }
}
