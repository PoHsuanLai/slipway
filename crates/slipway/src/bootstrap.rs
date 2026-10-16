//! Assembling the real collaborators for `slipway run`.

use crate::clock::jitter_seed;
use crate::config::Config;
use crate::dirs::Dirs;
use crate::elevate::{Helpers, Pkexec};
use crate::fetch::{Source, UreqFetch};
use crate::keys::load_key;
use crate::layout::Layout;
use crate::service::{Anchor, Context};
use crate::state::StateStore;
use slipway_core::Authenticator;
use std::sync::Arc;

const MACHINE_ID: &str = "/etc/machine-id";

/// The context for the real system: the configured root, the real key, ureq, pkexec.
pub fn production(config: Config, dirs: &Dirs) -> Context {
    let anchor: Anchor =
        load_key(&config.pubkey_path).map(|k| Arc::new(k) as Arc<dyn Authenticator + Send + Sync>);
    let seed = std::fs::read_to_string(MACHINE_ID).map_or(0, |id| jitter_seed(&id));
    Context {
        layout: Layout::new(config.root.clone()),
        cache_dir: dirs.cache_dir.clone(),
        store: StateStore::new(dirs.state_file.clone()),
        source: Source::new(&config.index_url),
        fetch: Arc::new(UreqFetch::new()),
        anchor,
        elevate: Arc::new(Pkexec::new(Helpers::from(&config))),
        login: crate::login::current(),
        seed,
        config,
    }
}
