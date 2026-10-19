use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use rusty_core::{FsError, FsErrorKind, TabId, relist_tab};

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
    pub(super) fn answered(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn has_answered(&self) -> bool {
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
    is_retrying: bool,
    retries: usize,
}

impl Stall {
    fn keeps_on(&self, error: &FsError) -> bool {
        if self.offline.is_some() {
            !ends_offline(error)
        } else {
            is_unreachable(error)
        }
    }
}

#[derive(Default)]
pub(super) struct Stalls(HashMap<TabId, Stall>);

impl Stalls {
    pub(super) fn begin(&mut self, tab: TabId, request: u64, progress: Progress) {
        let stall = Stall {
            request,
            progress,
            offline: None,
            is_attempting: true,
            is_retrying: false,
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

fn ends_offline(error: &FsError) -> bool {
    matches!(
        error.kind,
        FsErrorKind::NotFound
            | FsErrorKind::AccessDenied
            | FsErrorKind::NameInvalid
            | FsErrorKind::PathTooLong
    )
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
            .filter(|stall| stall.request == request && !stall.is_retrying)?;
        if stall.offline.is_none() && stall.progress.has_answered() {
            return None;
        }
        stall.offline.get_or_insert(Offline { cached_at });
        stall.is_retrying = true;
        Some(retry_delay(0))
    }

    pub fn take_new_offline(&mut self) -> Vec<(TabId, u64, Duration)> {
        let pending: Vec<(TabId, u64)> = self
            .stalls
            .0
            .iter()
            .filter(|(_, stall)| stall.offline.is_some() && !stall.is_retrying)
            .map(|(tab, stall)| (*tab, stall.request))
            .filter(|(tab, request)| self.is_pending(*tab, *request))
            .collect();
        for (tab, _) in &pending {
            if let Some(stall) = self.stalls.0.get_mut(tab) {
                stall.is_retrying = true;
            }
        }
        pending
            .into_iter()
            .map(|(tab, request)| (tab, request, retry_delay(0)))
            .collect()
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
        let cached_at = self.cached_at(tab);
        let error = result.as_ref().err();
        if let Some(stall) = self
            .stalls
            .0
            .get_mut(&tab)
            .filter(|stall| stall.request == request)
            && error.is_some_and(|error| stall.keeps_on(error))
        {
            stall.offline.get_or_insert(Offline { cached_at });
            stall.is_attempting = false;
            return true;
        }
        self.stalls.0.remove(&tab);
        false
    }

    pub(super) fn relist_answering(&mut self) {
        let tabs: Vec<TabId> = self
            .open_tabs()
            .into_iter()
            .map(|(tab, _)| tab)
            .filter(|tab| self.offline(*tab).is_none())
            .collect();
        let effects = tabs
            .into_iter()
            .flat_map(|tab| relist_tab(&mut self.state, tab))
            .collect();
        self.run(effects);
    }

    pub(super) fn retries_instead(&mut self) -> bool {
        let tab = self.state.active_tab().id();
        if self.offline(tab).is_none() {
            return false;
        }
        self.retry_listing(tab);
        true
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
