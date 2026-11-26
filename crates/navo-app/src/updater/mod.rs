pub(crate) mod apply;
pub(crate) mod feed;
pub(crate) mod root_access;
mod schedule;
pub(crate) mod service;
#[cfg(feature = "tester")]
pub(crate) mod test_feed;
mod view;

use std::sync::Arc;

use navo_store::StoreDirs;

use feed::VelopackFeed;
use service::{START_DELAY, Updater};

/// The updater of this copy, or `None` without an install or with an invalid tester feed (D22b).
pub(crate) fn launch(dirs: Option<&StoreDirs>) -> Option<Arc<Updater>> {
    let feed = VelopackFeed::detect().ok()?;
    Some(Updater::start_with_dirs(Arc::new(feed), START_DELAY, dirs))
}
