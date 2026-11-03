pub(crate) mod apply;
pub(crate) mod feed;
mod schedule;
pub(crate) mod service;
mod view;

use std::sync::Arc;

use feed::VelopackFeed;
use service::{START_DELAY, Updater};

/// The updater of this copy, or `None` when it has no install around it (D22b).
pub(crate) fn launch() -> Option<Arc<Updater>> {
    let feed = VelopackFeed::detect().ok()?;
    Some(Updater::start(Arc::new(feed), START_DELAY))
}
