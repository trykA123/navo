use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use navo_core::{Location, ProviderPath};
use navo_ui::git::FolderBadges;

use super::watches::root_key;
use super::{Cancel, QUEUE_BYTES, QUEUE_CAP, REPO_CAP, ROW_CAP, Row};

pub(super) struct Listing {
    pub folder: Location,
    pub path: PathBuf,
    pub generation: u64,
    pub rows: Vec<Row>,
    pub cancelled: Cancel,
    pub calls: Mutex<Vec<Cancel>>,
}

impl Listing {
    pub fn new(folder: &Location, generation: u64, mut rows: Vec<Row>) -> Option<Arc<Self>> {
        let path = navo_fs::os_path(folder).ok()?;
        rows.sort_by_key(|row| !row.on_screen);
        rows.truncate(ROW_CAP);
        let mut bytes = 0;
        rows.retain(|row| {
            bytes += row.name.capacity() + std::mem::size_of::<Row>();
            bytes <= QUEUE_BYTES / 4 && folder.join(&ProviderPath::from_text(&row.name)).is_ok()
        });
        rows.shrink_to_fit();
        Some(Arc::new(Self {
            folder: folder.clone(),
            path,
            generation,
            rows,
            cancelled: Cancel::new(),
            calls: Mutex::new(Vec::new()),
        }))
    }

    pub fn cancel(&self) {
        self.cancelled.cancel();
        for cancel in self
            .calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
        {
            cancel.cancel();
        }
    }

    pub fn start_call(&self) -> Cancel {
        let cancel = Cancel::new();
        let mut calls = self.calls.lock().unwrap_or_else(PoisonError::into_inner);
        calls.retain(|token| !token.is_cancelled());
        if self.cancelled.is_cancelled() {
            cancel.cancel();
        }
        calls.push(cancel.clone());
        cancel
    }

    fn bytes(&self) -> usize {
        self.rows
            .iter()
            .map(|r| r.name.capacity() + std::mem::size_of::<Row>())
            .sum::<usize>()
            + self.path.as_os_str().len() * 2
            + self.folder.as_str().len() * 4
            + 256
    }
}

#[derive(Clone)]
pub(super) enum Request {
    Probe(Arc<Listing>),
    Status(Arc<Listing>, PathBuf),
}

impl Request {
    pub fn listing(&self) -> &Arc<Listing> {
        match self {
            Self::Probe(listing) | Self::Status(listing, _) => listing,
        }
    }
}

/// What a rescan asks for: only changed roots, every root, or one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Scope {
    Changed,
    All,
    Repo(PathBuf),
}

/// Roots a scan must check regardless of their proof, by root key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Forced {
    pub all: bool,
    pub roots: Vec<String>,
}

impl Forced {
    fn apply(&mut self, scope: Scope) {
        match scope {
            // Changed adds nothing: it is the normal plan, and explicit forces stay.
            Scope::Changed => {}
            Scope::All => {
                self.all = true;
                self.roots.clear();
            }
            Scope::Repo(path) => {
                if self.all {
                    return;
                }
                let key = root_key(&path);
                if !self.roots.contains(&key) {
                    self.roots.push(key);
                }
                // More forced roots than discovery can hold is every root.
                if self.roots.len() > REPO_CAP {
                    self.all = true;
                    self.roots.clear();
                }
            }
        }
    }
}

pub(super) struct Entry {
    pub listing: Arc<Listing>,
    pub badges: Option<Arc<FolderBadges>>,
    pub forced: Forced,
}

#[derive(Default)]
pub(super) struct Shared {
    pub queue: VecDeque<Request>,
    pub active: HashMap<Location, Entry>,
    pub stopped: bool,
    /// The worker sleeps until a request or a stop wakes it.
    #[cfg(test)]
    pub parked: bool,
    revision: u64,
    order: VecDeque<Location>,
}

impl Shared {
    pub fn observe(&mut self, listing: Arc<Listing>) {
        if self.stopped
            || self
                .active
                .get(&listing.folder)
                .is_some_and(|old| old.listing.generation >= listing.generation)
        {
            return;
        }
        let old = self.active.get(&listing.folder);
        let kept = old.and_then(|old| old.badges.clone());
        let forced = old.map(|old| old.forced.clone()).unwrap_or_default();
        self.replace(listing, kept, forced);
    }

    /// Swaps in a listing; the old badges stay visible until the new scan publishes.
    fn replace(
        &mut self,
        listing: Arc<Listing>,
        badges: Option<Arc<FolderBadges>>,
        forced: Forced,
    ) {
        self.forget(&listing.folder);
        self.order.push_back(listing.folder.clone());
        self.active.insert(
            listing.folder.clone(),
            Entry {
                listing: listing.clone(),
                badges,
                forced,
            },
        );
        while self.active.len() > QUEUE_CAP || self.bytes() > QUEUE_BYTES {
            let Some(oldest) = self.order.front().cloned() else {
                break;
            };
            self.forget(&oldest);
        }
        if self.is_current(&listing) {
            self.push(Request::Probe(listing));
        }
    }

    pub fn push(&mut self, request: Request) {
        while self.queue.len() >= QUEUE_CAP || self.bytes() + request_bytes(&request) > QUEUE_BYTES
        {
            let oldest = self
                .order
                .iter()
                .find(|folder| self.queue.iter().any(|q| &q.listing().folder == *folder))
                .cloned();
            let Some(oldest) = oldest else {
                return;
            };
            self.queue.retain(|q| q.listing().folder != oldest);
        }
        self.queue.push_back(request);
    }

    pub fn push_all(&mut self, listing: &Arc<Listing>, roots: &[PathBuf]) {
        for root in roots {
            self.push(Request::Status(listing.clone(), root.clone()));
        }
    }

    pub fn pop(&mut self, has_slot: bool) -> Option<Request> {
        let index = self
            .queue
            .iter()
            .position(|q| has_slot || matches!(q, Request::Probe(_)))?;
        self.queue.remove(index)
    }

    pub fn forget(&mut self, folder: &Location) {
        if let Some(old) = self.active.remove(folder) {
            old.listing.cancel();
        }
        self.order.retain(|old| old != folder);
        self.queue.retain(|q| &q.listing().folder != folder);
    }

    pub fn rescan(&mut self, folder: &Location) {
        self.rescan_with(folder, Scope::All);
    }

    /// Rechecks only the roots whose watch counted a change.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "focus picks this scope in a later atom")
    )]
    pub fn rescan_changed(&mut self, folder: &Location) {
        self.rescan_with(folder, Scope::Changed);
    }

    /// Rechecks one repository's root; the scanner forces all when it matches no root.
    pub fn rescan_repo(&mut self, folder: &Location, path: PathBuf) {
        self.rescan_with(folder, Scope::Repo(path));
    }

    fn rescan_with(&mut self, folder: &Location, scope: Scope) {
        let Some(old) = self.active.get(folder) else {
            return;
        };
        let listing = Listing::new(folder, old.listing.generation, old.listing.rows.clone());
        let kept = old.badges.clone();
        let mut forced = old.forced.clone();
        forced.apply(scope);
        if self.stopped {
            return;
        }
        if let Some(listing) = listing {
            self.replace(listing, kept, forced);
        } else {
            self.forget(folder);
        }
    }

    /// Takes the roots the current scan must force, clearing them.
    pub fn take_forced(&mut self, listing: &Arc<Listing>) -> Forced {
        if !self.is_current(listing) {
            return Forced::default();
        }
        self.active
            .get_mut(&listing.folder)
            .map(|entry| std::mem::take(&mut entry.forced))
            .unwrap_or_default()
    }

    /// The folder already shows badges that a new scan will replace.
    pub fn shows_badges(&self, listing: &Arc<Listing>) -> bool {
        self.active
            .get(&listing.folder)
            .is_some_and(|entry| entry.badges.is_some())
    }

    pub fn is_current(&self, listing: &Arc<Listing>) -> bool {
        !self.stopped
            && !listing.cancelled.is_cancelled()
            && self
                .active
                .get(&listing.folder)
                .is_some_and(|entry| Arc::ptr_eq(&entry.listing, listing))
    }

    pub fn publish(&mut self, listing: &Arc<Listing>, mut badges: FolderBadges) -> Option<u64> {
        if !self.is_current(listing) {
            return None;
        }
        self.revision = self.revision.saturating_add(1).max(listing.generation);
        badges.revision = self.revision;
        let entry = self.active.get_mut(&listing.folder)?;
        entry.badges = Some(Arc::new(badges));
        Some(self.revision)
    }

    /// Later publishes number above `revision`, so a restarted scanner never repeats a revision.
    pub fn resume_after(&mut self, revision: u64) {
        self.revision = self.revision.max(revision);
    }

    pub fn stop(&mut self) {
        self.stopped = true;
        for entry in self.active.values() {
            entry.listing.cancel();
        }
        self.queue.clear();
        self.active.clear();
        self.order.clear();
    }

    fn bytes(&self) -> usize {
        self.active
            .values()
            .map(|entry| entry.listing.bytes())
            .sum::<usize>()
            + QUEUE_CAP * (std::mem::size_of::<Request>() + 128)
            + self.queue.iter().map(request_bytes).sum::<usize>()
    }
}

fn request_bytes(request: &Request) -> usize {
    match request {
        Request::Probe(_) => 0,
        Request::Status(_, root) => root.as_os_str().len() * 2 + 128,
    }
}

#[cfg(test)]
#[path = "scan_tests/queue.rs"]
mod tests;
