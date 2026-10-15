//! What the service says about itself: the properties of the bus object.

/// Where the update flow is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Nothing to do.
    Idle,
    /// Asking the release site what is newest.
    Checking,
    /// A newer version exists and is waiting for the person.
    Available,
    /// Downloading it.
    Downloading,
    /// Downloaded and verified, about to be installed.
    Ready,
    /// Switching to it.
    Installing,
    /// Installed; a new login starts it.
    Installed,
    /// The last thing tried did not work; see the problem.
    Failed,
}

impl Phase {
    /// The word on the bus.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Checking => "checking",
            Self::Available => "available",
            Self::Downloading => "downloading",
            Self::Ready => "ready",
            Self::Installing => "installing",
            Self::Installed => "installed",
            Self::Failed => "failed",
        }
    }

    /// Whether work is in progress, so a new request must wait.
    pub fn is_busy(self) -> bool {
        matches!(
            self,
            Self::Checking | Self::Downloading | Self::Ready | Self::Installing
        )
    }
}

/// A snapshot of every property.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// Where the flow is.
    pub phase: Phase,
    /// The installed version, "" when unknown.
    pub current: String,
    /// The newer version on offer, "" when none.
    pub available: String,
    /// Its notes.
    pub notes: String,
    /// Its download size in bytes.
    pub size: u64,
    /// Download progress in permille.
    pub progress: u32,
    /// When the last check finished (unix seconds), 0 never.
    pub last_checked: i64,
    /// What went wrong, in plain words, "" when nothing.
    pub problem: String,
}

impl Status {
    /// The status of a service that has just started.
    pub fn starting(current: String, last_checked: i64) -> Self {
        Self {
            phase: Phase::Idle,
            current,
            available: String::new(),
            notes: String::new(),
            size: 0,
            progress: 0,
            last_checked,
            problem: String::new(),
        }
    }
}
