//! Pane compare mode over the session thread: `CompareHooks` for the window.

use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, OnceLock};
use std::thread;

use navo_compare::model::HashCache;
use navo_core::Location;
use navo_store::CacheDb;
use navo_ui::browser::{CompareControl, CompareHooks, CompareSink, CompareUpdate};
use navo_ui::settings::Values;

use super::convert::{settings_of, update_of};
use super::handle::{CompareEvent, CompareHandle, CompareStart, CompareTarget};
use super::hash_cache::StoreHashCache;

/// Starts folder compares of two local panes. Hashes are kept for this run of Navo only.
#[derive(Default)]
pub(crate) struct PaneCompare {
    cache: OnceLock<Arc<dyn HashCache>>,
}

impl PaneCompare {
    #[cfg(test)]
    pub(super) fn with_cache(cache: Arc<dyn HashCache>) -> Self {
        Self {
            cache: OnceLock::from(cache),
        }
    }
}

impl CompareHooks for PaneCompare {
    /// Only local folders; `CompareTarget::Local` reads nothing else.
    fn supports(&self, location: &Location) -> bool {
        location.is_local()
    }

    fn start(
        &self,
        (left, right): (&Location, &Location),
        values: &Values,
        sink: CompareSink,
    ) -> Box<dyn CompareControl> {
        let (events, receiver) = mpsc::channel();
        let request = CompareStart {
            left: CompareTarget::Local(left.clone()),
            right: CompareTarget::Local(right.clone()),
            settings: settings_of(values),
            cache: Arc::clone(self.cache.get_or_init(memory_cache)),
        };
        forward(receiver, sink);
        Box::new(Running(CompareHandle::start(request, events)))
    }
}

/// Drains the session's events onto the UI sink until the session thread ends.
fn forward(receiver: Receiver<CompareEvent>, sink: CompareSink) {
    let failed = Arc::clone(&sink);
    let spawned = thread::Builder::new()
        .name("compare-forward".to_owned())
        .spawn(move || {
            for event in receiver {
                sink(update_of(event));
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "compare forward thread did not start");
        failed(CompareUpdate::Failed(navo_l10n::tr(
            "compare-could-not-start",
        )));
    }
}

/// Dropping it drops the handle, which cancels and joins off the UI thread.
struct Running(CompareHandle);

impl CompareControl for Running {
    fn focus(&self, row_ids: Vec<u64>) {
        self.0.focus(row_ids);
    }

    fn stop_row(&self, id: u64) {
        self.0.stop_row(id);
    }

    fn stop_all(&self) {
        self.0.stop_all();
    }

    fn refresh(&self) {
        self.0.refresh();
    }

    fn continue_walk(&self) {
        self.0.continue_walk();
    }
}

fn memory_cache() -> Arc<dyn HashCache> {
    match CacheDb::open_in_memory() {
        Ok(db) => Arc::new(StoreHashCache::new(db)),
        Err(error) => {
            tracing::warn!(%error, "compare hash cache unavailable");
            Arc::new(NoHashes)
        }
    }
}

struct NoHashes;

impl HashCache for NoHashes {
    fn get(&self, _key: &navo_compare::model::ContentKey) -> Option<[u8; 32]> {
        None
    }

    fn put(&self, _key: &navo_compare::model::ContentKey, _hash: [u8; 32]) {}
}
