//! What the engine needs from outside, gathered in one place so tests can swap the parts.

use crate::config::Config;
use crate::elevate::Elevate;
use crate::fetch::{Fetch, Source};
use crate::keys::KeyError;
use crate::layout::Layout;
use crate::state::StateStore;
use slipway_core::{Authenticator, LoginSession};
use std::path::PathBuf;
use std::sync::Arc;

/// The trust anchor, or why there is none.
pub type Anchor = Result<Arc<dyn Authenticator + Send + Sync>, KeyError>;

/// The engine's collaborators.
pub struct Context {
    /// The settings.
    pub config: Config,
    /// The install root and export base.
    pub layout: Layout,
    /// Where releases are downloaded to.
    pub cache_dir: PathBuf,
    /// The daemon's memory.
    pub store: StateStore,
    /// Where release files come from.
    pub source: Source,
    /// How they are downloaded.
    pub fetch: Arc<dyn Fetch>,
    /// The key that signs releases.
    pub anchor: Anchor,
    /// How the privileged helper is run.
    pub elevate: Arc<dyn Elevate>,
    /// The login this daemon runs in (the health marker's notion of "the same session").
    pub login: LoginSession,
    /// A number that spreads this computer's checks over the hour.
    pub seed: u64,
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("layout", &self.layout)
            .finish_non_exhaustive()
    }
}
