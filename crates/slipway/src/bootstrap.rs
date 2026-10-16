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
use peephole_bus::peephole::{RealProc, TableFiles};
use peephole_bus::peephole_core::Table;
use slipway_core::Authenticator;
use std::sync::Arc;

const MACHINE_ID: &str = "/etc/machine-id";

/// The desktop's table of its own programs from `/etc/quire`. A table that is missing or that cannot
/// be trusted (not root-only, not a table) is an empty one: no caller then counts as the shell, Settings
/// or the `slipway` program, and the service says why in its log.
fn desktop_table() -> Table {
    TableFiles::desktop()
        .load(&RealProc::system())
        .unwrap_or_else(|why| {
            tracing::warn!("the table of the desktop's own programs was not used: {why}");
            Table::empty()
        })
}

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
        callers: desktop_table(),
        seed,
        config,
    }
}
