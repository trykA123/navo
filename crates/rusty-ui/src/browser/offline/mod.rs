use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use rusty_core::{FsError, FsErrorKind, TabId};

use super::Browser;
use super::cached::Fresh;

const RETRY_AFTER: [Duration; 4] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(30),
];

#[must_use]
pub(super) fn retry_delay(retries: usize) -> Duration {
    RETRY_AFTER[retries.min(RETRY_AFTER.len() - 1)]
}

#[derive(Clone, Default)]
pub(super) struct Progress(Arc<AtomicBool>);

impl Progress {
    pub(super) fn batch_arrived(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn has_batches(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offline {
    pub cached_at: Option<SystemTime>,
}

struct Stall {
    request: u64,
    progress: Progress,
    offline: Option<Offline>,
    is_attempting: bool,
    retries: usize,
}

#[derive(Default)]
pub(super) struct Stalls(HashMap<TabId, Stall>);

impl Stalls {
    pub(super) fn begin(&mut self, tab: TabId, request: u64) {
        let stall = Stall {
            request,
            progress: Progress::default(),
            offline: None,
            is_attempting: true,
            retries: 0,
        };
        self.0.insert(tab, stall);
    }

    pub(super) fn progress(&self, tab: TabId) -> Progress {
        self.0
            .get(&tab)
            .map(|stall| stall.progress.clone())
            .unwrap_or_default()
    }

    pub(super) fn retain(&mut self, is_open: impl Fn(&TabId) -> bool) {
        self.0.retain(|tab, _| is_open(tab));
    }

    fn offline_mut(&mut self, tab: TabId, request: u64) -> Option<&mut Stall> {
        self.0
            .get_mut(&tab)
            .filter(|stall| stall.request == request && stall.offline.is_some())
    }
}

fn is_unreachable(error: &FsError) -> bool {
    matches!(error.kind, FsErrorKind::NetworkLost | FsErrorKind::Timeout)
}

impl Browser {
    #[must_use]
    pub fn offline(&self, tab: TabId) -> Option<Offline> {
        let stall = self.stalls.0.get(&tab)?;
        stall
            .offline
            .filter(|_| self.is_pending(tab, stall.request))
    }

    pub fn listing_overdue(&mut self, tab: TabId, request: u64) -> Option<Duration> {
        let cached_at = self.cached_at(tab);
        let stall = self
            .stalls
            .0
            .get_mut(&tab)
            .filter(|stall| stall.request == request && stall.offline.is_none())?;
        if stall.progress.has_batches() {
            return None;
        }
        stall.offline = Some(Offline { cached_at });
        Some(retry_delay(0))
    }

    pub fn retry_offline(&mut self, tab: TabId, request: u64) -> Option<Duration> {
        let stall = self.stalls.offline_mut(tab, request)?;
        stall.retries += 1;
        let next = retry_delay(stall.retries);
        if self.open_tabs().iter().any(|(open, _)| *open == tab) {
            self.attempt(tab, request);
        }
        Some(next)
    }

    pub fn retry_listing(&mut self, tab: TabId) -> bool {
        let Some(request) = self.state.tab(tab).and_then(|t| t.pending_request()) else {
            return false;
        };
        let Some(stall) = self.stalls.offline_mut(tab, request) else {
            return false;
        };
        stall.retries = 0;
        self.attempt(tab, request)
    }

    pub(super) fn keeps_offline(
        &mut self,
        tab: TabId,
        request: u64,
        result: &Result<Fresh, FsError>,
    ) -> bool {
        let is_unanswered = result.as_ref().err().is_some_and(is_unreachable);
        match self.stalls.offline_mut(tab, request) {
            Some(stall) if is_unanswered => {
                stall.is_attempting = false;
                true
            }
            _ => {
                self.stalls.0.remove(&tab);
                false
            }
        }
    }

    fn attempt(&mut self, tab: TabId, request: u64) -> bool {
        let Some(location) = self.state.tab(tab).map(|t| t.location().clone()) else {
            return false;
        };
        let Some(stall) = self
            .stalls
            .offline_mut(tab, request)
            .filter(|stall| !stall.is_attempting)
        else {
            return false;
        };
        stall.is_attempting = true;
        self.submit(tab, location, request);
        true
    }

    fn is_pending(&self, tab: TabId, request: u64) -> bool {
        self.state
            .tab(tab)
            .is_some_and(|t| t.pending_request() == Some(request))
    }

    fn cached_at(&self, tab: TabId) -> Option<SystemTime> {
        let shown = self.state.tab(tab).filter(|t| t.listing().is_some())?;
        let key = self.keys.key(shown.location())?;
        self.cache.get(&key).map(|listing| listing.read_at)
    }
}
