use std::sync::Arc;

use rusty_cache::CachedListing;
use rusty_core::TabId;
use rusty_fs::{CancelToken, FsEntry};

use super::{Sorted, WorkerSorted};
use crate::browser::Browser;
use crate::browser::filter::Full;
use crate::browser::sort::sorted_full;
use crate::file_list::SortKey;

pub(in crate::browser) struct CachedSorted {
    pub(super) tab: TabId,
    pub(super) request: u64,
    pub(super) rows: Arc<[FsEntry]>,
    pub(super) sorted: WorkerSorted,
}

pub(in crate::browser) struct Early {
    pub(super) cached: Arc<CachedListing>,
    pub(super) sort: (SortKey, bool),
    pub(super) notify: Box<dyn FnOnce(Arc<[FsEntry]>, WorkerSorted) + Send>,
}

impl Early {
    pub(super) fn send(self, cancel: &CancelToken) {
        let (key, folders_first) = self.sort;
        let rows = Arc::clone(&self.cached.rows);
        let full = sorted_full(&rows, key, folders_first);
        let sorted = WorkerSorted {
            key,
            folders_first,
            full,
        };
        if !cancel.is_cancelled() {
            (self.notify)(rows, sorted);
        }
    }
}

impl Browser {
    pub(super) fn adopt(
        &mut self,
        tab: TabId,
        rows: &Arc<[FsEntry]>,
        worker: Option<WorkerSorted>,
    ) -> Full {
        let current = (self.sort_key(tab), self.folders_first);
        match worker {
            Some(sorted) if (sorted.key, sorted.folders_first) == current => {
                self.remember(
                    tab,
                    Sorted {
                        source: Arc::clone(rows),
                        key: sorted.key,
                        full: sorted.full.clone(),
                    },
                );
                sorted.full
            }
            _ => self.sorted_full(tab, rows),
        }
    }

    pub(in crate::browser) fn accept_cached_sorted(&mut self, early: CachedSorted) -> bool {
        let CachedSorted {
            tab,
            request,
            rows,
            sorted,
        } = early;
        let is_current = self
            .state
            .tab(tab)
            .is_some_and(|t| t.pending_request() == Some(request) && t.listing().is_none());
        if !is_current {
            return false;
        }
        let full = self.adopt(tab, &rows, Some(sorted));
        self.show_cached_full(tab, request, full)
    }
}
