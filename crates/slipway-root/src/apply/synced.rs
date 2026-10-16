//! What syncing the exports or the system files left to tell the person.

/// The outcome of one sync step.
#[derive(Debug, Default)]
pub struct Synced {
    /// Destinations left alone because something else is there.
    pub conflicts: Vec<String>,
    /// Plain notes about things that did not work but did not stop the install.
    pub warnings: Vec<String>,
}
