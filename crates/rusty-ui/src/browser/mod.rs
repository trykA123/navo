use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use rusty_cache::{CacheKeys, ListingCache, ProfileCache};
use rusty_core::{
    AppState, Command, CommandError, CursorMove, Effect, Event, FsError, Listing, Location,
    ProviderPath, TabId, apply, apply_event, list_missing,
};
use rusty_fs::{
    EntryKind, FsEntry, IoQueue, JobSlot, OwnerGuard, Priority, ProviderOps, WatchEvent, volume_of,
};

pub(crate) use measure::Measured;
pub use naming::NameProblem;
pub use offline::Offline;
use operate::JobUpdate;
pub use operate::{JobStatus, JobView, LogFactory, OpLog, Operations, PendingOp};

pub type Waker = Arc<dyn Fn() + Send + Sync>;
pub type Launcher = Arc<dyn Fn(&Location) + Send + Sync>;

struct Loaded {
    tab: TabId,
    location: Location,
    request: u64,
    result: Result<cached::Fresh, FsError>,
}

enum Message {
    Loaded(Loaded),
    Measured(measure::Measured),
    Job(JobUpdate),
    Named(naming::Named),
    Watched(Location, WatchEvent),
    Recheck(Location),
}

impl Message {
    fn is_queued(&self) -> bool {
        !matches!(
            self,
            Self::Job(JobUpdate::Progress { .. } | JobUpdate::Done { .. })
                | Self::Watched(..)
                | Self::Recheck(_)
        )
    }
}

pub struct Browser {
    state: AppState,
    provider: Arc<dyn ProviderOps>,
    queue: IoQueue,
    owner: OwnerGuard,
    sizes: IoQueue,
    sizes_owner: OwnerGuard,
    sender: Sender<Message>,
    receiver: Receiver<Message>,
    wake: Waker,
    details: HashMap<TabId, Rc<[FsEntry]>>,
    errors: HashMap<TabId, FsError>,
    late_errors: HashMap<TabId, FsError>,
    focus_after_load: HashMap<TabId, String>,
    in_flight: usize,
    save_requested: bool,
    operations: Operations,
    pending: Option<PendingOp>,
    jobs: Vec<JobView>,
    ended: Vec<u64>,
    filters: HashMap<TabId, filter::Filter>,
    full: HashMap<TabId, filter::Full>,
    show_hidden: bool,
    sorts: HashMap<TabId, crate::file_list::SortKey>,
    next_op: u64,
    launcher: Launcher,
    cache: ListingCache,
    keys: CacheKeys,
    cached: HashMap<TabId, u64>,
    pins: HashSet<rusty_cache::CacheKey>,
    sorted: HashMap<TabId, Vec<cached::Sorted>>,
    watching: watched::Watching,
    profiles: ProfileCache,
    stalls: offline::Stalls,
}

impl Browser {
    pub fn new(state: AppState, provider: Arc<dyn ProviderOps>, wake: Waker) -> Self {
        let queue = IoQueue::new();
        let owner = queue.owner();
        let sizes = IoQueue::new();
        let sizes_owner = sizes.owner();
        let (sender, receiver) = mpsc::channel();
        Self {
            state,
            provider,
            queue,
            owner,
            sizes,
            sizes_owner,
            sender,
            receiver,
            wake,
            details: HashMap::new(),
            errors: HashMap::new(),
            late_errors: HashMap::new(),
            focus_after_load: HashMap::new(),
            in_flight: 0,
            save_requested: false,
            operations: Operations::unlogged(),
            pending: None,
            jobs: Vec::new(),
            ended: Vec::new(),
            filters: HashMap::new(),
            full: HashMap::new(),
            show_hidden: true,
            sorts: HashMap::new(),
            next_op: 0,
            launcher: Arc::new(|_| {}),
            cache: ListingCache::default(),
            keys: CacheKeys::default(),
            cached: HashMap::new(),
            pins: HashSet::new(),
            sorted: HashMap::new(),
            watching: watched::Watching::default(),
            profiles: ProfileCache::default(),
            stalls: offline::Stalls::default(),
        }
    }

    pub fn start(&mut self) {
        let effects = list_missing(&mut self.state);
        self.run(effects);
    }

    pub fn command(&mut self, command: &Command) -> Result<(), CommandError> {
        if matches!(command, Command::PaneRefresh) && self.retries_instead() {
            return Ok(());
        }
        let focused = self.state.focused_pane();
        let effects = apply(&mut self.state, command)?;
        self.drop_stale_filters();
        self.run(effects);
        if self.state.focused_pane() != focused {
            self.revalidate_focused();
        }
        Ok(())
    }

    pub fn event(&mut self, event: Event) {
        let effects = apply_event(&mut self.state, event);
        self.run(effects);
    }

    pub fn open_row(&mut self, row: u64) -> bool {
        let tab = self.state.active_tab();
        let Some(entry) = tab.listing().and_then(|l| l.get(row)) else {
            return false;
        };
        let is_link = usize::try_from(row)
            .ok()
            .and_then(|index| self.details.get(&tab.id())?.get(index).cloned())
            .is_some_and(|detail| {
                detail.kind == EntryKind::Link && detail.attributes.link_to_folder
            });
        let Ok(location) = tab.location().join(&ProviderPath::from_text(&entry.name)) else {
            return false;
        };
        if !entry.is_folder && !is_link {
            (self.launcher)(&location);
            return true;
        }
        self.command(&Command::PaneNavigate { location }).is_ok()
    }

    pub fn set_launcher(&mut self, launcher: Launcher) {
        self.launcher = launcher;
    }

    pub fn go_up(&mut self) -> bool {
        let tab = self.state.active_tab();
        let Some(parent) = tab.location().parent() else {
            return false;
        };
        let child = tab
            .location()
            .path()
            .to_text()
            .rsplit(['/', '\\'])
            .find(|segment| !segment.is_empty())
            .map(str::to_owned);
        let id = tab.id();
        if self
            .command(&Command::PaneNavigate { location: parent })
            .is_err()
        {
            return false;
        }
        if let Some(child) = child {
            self.focus_after_load.insert(id, child);
        }
        true
    }

    pub fn open_at(&mut self, folder: Location, name: String) -> bool {
        let id = self.state.active_tab().id();
        if self
            .command(&Command::PaneNavigate { location: folder })
            .is_err()
        {
            return false;
        }
        self.focus_after_load.insert(id, name);
        true
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(message) = self.receiver.try_recv() {
            if message.is_queued() {
                self.in_flight = self.in_flight.saturating_sub(1);
            }
            changed |= match message {
                Message::Loaded(loaded) => self.accept(loaded),
                Message::Measured(measured) => self.show_folder_size(&measured),
                Message::Job(update) => self.accept_job(update),
                Message::Named(named) => self.accept_named(named),
                Message::Watched(location, event) => self.accept_watched(&location, event),
                Message::Recheck(location) => self.accept_recheck(&location),
            };
        }
        changed
    }

    #[must_use]
    pub fn profiles(&self) -> &ProfileCache {
        &self.profiles
    }

    #[must_use]
    pub fn loading(&self) -> bool {
        self.in_flight > 0
    }

    #[must_use]
    pub fn state(&self) -> &AppState {
        &self.state
    }

    #[must_use]
    pub fn details(&self, tab: TabId) -> Rc<[FsEntry]> {
        self.details
            .get(&tab)
            .filter(|_| self.is_loaded(tab))
            .cloned()
            .unwrap_or_else(|| Rc::from([]))
    }

    #[must_use]
    pub fn error(&self, tab: TabId) -> Option<&FsError> {
        self.errors.get(&tab).filter(|_| self.is_loaded(tab))
    }

    fn is_loaded(&self, tab: TabId) -> bool {
        self.state.tab(tab).is_some_and(|t| t.listing().is_some())
    }

    pub fn take_save_request(&mut self) -> bool {
        std::mem::take(&mut self.save_requested)
    }

    fn run(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::ListFolder {
                    tab,
                    location,
                    request,
                } => self.list(tab, location, request),
                Effect::SaveSession => self.save_requested = true,
                Effect::MeasureFolder {
                    tab,
                    location,
                    name,
                } => self.measure(tab, location, name),
            }
        }
        self.sync_pins();
        self.sync_disk_folders();
        self.sync_watches();
    }

    fn list(&mut self, tab: TabId, location: Location, request: u64) {
        self.stalls.begin(tab, request);
        self.submit(tab, location, request);
    }

    fn submit(&mut self, tab: TabId, location: Location, request: u64) {
        let fetch = self.fetch(tab, &location, request);
        let (sender, wake) = (self.sender.clone(), Arc::clone(&self.wake));
        let slot = JobSlot {
            volume: volume_of(&location),
            priority: Priority::Visible,
            owner: self.owner.id(),
        };
        self.in_flight += 1;
        self.queue.submit(
            slot,
            Box::new(move |cancel| {
                let result = fetch.run(cancel);
                let _ = sender.send(Message::Loaded(Loaded {
                    tab,
                    location,
                    request,
                    result,
                }));
                wake();
            }),
        );
    }

    fn accept(&mut self, loaded: Loaded) -> bool {
        let Loaded {
            tab,
            location,
            request,
            result,
        } = loaded;
        let current = self
            .state
            .tab(tab)
            .is_some_and(|t| t.pending_request() == Some(request));
        if !current {
            return false;
        }
        if self.keeps_offline(tab, request, &result) {
            return true;
        }
        let listing = match result {
            Ok(fresh) => {
                match self.late_errors.remove(&tab) {
                    Some(error) => self.errors.insert(tab, error),
                    None => self.errors.remove(&tab),
                };
                self.fresh_listing(tab, &location, fresh)
            }
            Err(error) => {
                self.errors.insert(tab, error);
                self.details.remove(&tab);
                self.full.remove(&tab);
                Listing::new(Vec::new())
            }
        };
        self.event(Event::ListingLoaded {
            tab,
            location,
            request,
            listing,
        });
        self.cached.remove(&tab);
        self.restore_focus(tab);
        self.drain_refresh(tab);
        true
    }

    fn restore_focus(&mut self, tab: TabId) {
        let Some(name) = self.focus_after_load.remove(&tab) else {
            return;
        };
        let row = self
            .state
            .tab(tab)
            .and_then(|t| t.listing())
            .and_then(|l| l.entries().iter().position(|e| e.name == name));
        let active = self.state.active_tab().id() == tab;
        if let (Some(row), true) = (row, active) {
            let to = CursorMove::Row(row as u64);
            let _ = self.command(&Command::PaneCursorMove { to });
        }
    }
}

mod cached;
mod filter;
mod measure;
mod naming;
mod offline;
mod operate;
mod sort;
#[cfg(test)]
mod tests;
mod watched;
