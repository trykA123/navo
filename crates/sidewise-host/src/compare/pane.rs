//! Pane compare mode over the session thread: `CompareHooks` for the window.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use navo_compare::model::HashCache;
use navo_core::Location;
use navo_ui::browser::{CompareControl, CompareHooks, CompareSink, CompareUpdate};
use navo_ui::compare_diff::CompareOptions;

use super::convert::{settings_from, updates_of};
use super::handle::{CompareEvent, CompareHandle, CompareStart, CompareTarget};
use super::hash_cache::policy::RecheckHashes;

/// Turns a pane's location into a compare target. The host decides what a location opens as.
pub type LocalTarget = Arc<dyn Fn(&Location) -> CompareTarget + Send + Sync>;

/// Starts folder compares of two panes. The host picks the hash cache and the target builder.
pub struct PaneCompare {
    cache: Arc<dyn HashCache>,
    local: LocalTarget,
}

impl PaneCompare {
    /// Builds the hooks over the host's hash cache and target builder, shared by every compare it starts.
    pub fn new(cache: Arc<dyn HashCache>, local: LocalTarget) -> Self {
        Self { cache, local }
    }
}

impl CompareHooks for PaneCompare {
    /// Only local folders; the host's `LocalTarget` opens them.
    fn supports(&self, location: &Location) -> bool {
        location.is_local()
    }

    fn start(
        &self,
        (left, right): (&Location, &Location),
        options: &CompareOptions,
        sink: CompareSink,
    ) -> Box<dyn CompareControl> {
        let (events, receiver) = mpsc::channel();
        let cache: Arc<dyn HashCache> = if options.fresh_hashes {
            Arc::new(RecheckHashes(Arc::clone(&self.cache)))
        } else {
            Arc::clone(&self.cache)
        };
        let request = CompareStart {
            left: (self.local)(left),
            right: (self.local)(right),
            settings: settings_from(options),
            cache,
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
                for update in updates_of(event) {
                    sink(update);
                }
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
