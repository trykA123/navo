use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use navo_cache::{CacheKeys, ListingCache, ProfileCache};
use navo_core::placement::{MonitorSetup, Placement};
use navo_core::{
    AppState, Command, CommandError, CursorMove, Effect, Event, FsError, Listing, Location, PaneId,
    TabId, apply_event, list_missing,
};
use navo_fs::view::Rows;
use navo_fs::{FsEntry, JobKind, JobSlot, OwnerGuard, Pools, Priority, ProviderOps, volume_of};

pub use compare::{
    BarView, CompareControl, CompareFilter, CompareHooks, CompareKind, CompareMode, CompareRefusal,
    CompareRow, CompareSide, CompareSink, CompareTotals, CompareUpdate, ExitStep, FILTERS,
    FileDiffRequest, FolderSession, RowProgress, RowStatus, RowView, SideView, StopBound, Tone,
    Total, bar_view, empty_text, pane_rows,
};
pub(crate) use compare::{look_of, status_text};
pub use find::FindResults;
pub use measure::{FolderTotals, FolderTotalsQuery, SizeMode, no_folder_totals};
pub(crate) use measure::{Measured, folder_bytes};
use message::Message;
pub(crate) use open::Seen;
pub use operate::*;
use rename_batch::run_batch;
pub use rename_batch::{
    DeletePreset, LastRename, ListPresets, NamedPreset, RenameHooks, RenameLogStatus,
    RenamePresetList, RenameReport, RetireUndo, SavePreset, UndoKind, UndoLogFactory, UndoOffer,
    WriteRenameLog, run_undo,
};
pub use retained::{NoRetained, RetainedHooks, RetainedKind, RetainedRow, RetainedState};
pub use {naming::NameProblem, offline::Offline};

pub type Waker = Arc<dyn Fn() + Send + Sync>;
pub type Launcher = Arc<dyn Fn(&Location) + Send + Sync>;

struct Loaded {
    tab: TabId,
    location: Location,
    request: u64,
    result: Result<cached::Fresh, FsError>,
}

pub struct Browser {
    state: AppState,
    provider: Arc<dyn ProviderOps>,
    pools: &'static Pools,
    owner: OwnerGuard,
    size_owners: HashMap<TabId, OwnerGuard>,
    finds: HashMap<TabId, find::FindResults>,
    find_walk: find::FindWalk,
    sizes: measure::SizeView,
    sender: Sender<Message>,
    receiver: Receiver<Message>,
    wake: Waker,
    details: HashMap<TabId, Rows>,
    errors: HashMap<TabId, FsError>,
    late_errors: HashMap<TabId, FsError>,
    focus_after_load: HashMap<TabId, String>,
    in_flight: usize,
    save_requested: bool,
    operations: Operations,
    pending: Option<PendingOp>,
    jobs: Vec<JobView>,
    job_queue: operate::queue::JobQueue,
    ended: Vec<u64>,
    filters: HashMap<TabId, filter::Filter>,
    hidden_marks: HashMap<TabId, filter::HiddenMarks>,
    full: HashMap<TabId, filter::Full>,
    shows: filter::Visibility,
    measures_folders: bool,
    folder_totals: FolderTotalsQuery,
    default_sort: crate::file_list::SortKey,
    folders_first: bool,
    copying: operate::Copying,
    sorts: HashMap<TabId, crate::file_list::SortKey>,
    sort_count: sort::SortCount,
    next_op: u64,
    launcher: Launcher,
    seen: Vec<open::Seen>,
    shown: HashMap<TabId, Location>,
    cache: ListingCache,
    keys: CacheKeys,
    cached: HashMap<TabId, u64>,
    pins: HashSet<navo_cache::CacheKey>,
    sorted: HashMap<TabId, Vec<cached::Sorted>>,
    watching: watched::Watching,
    profiles: ProfileCache,
    stalls: offline::Stalls,
    prefetches: prefetch::Prefetches,
    hooks: retained::HookSlots,
    view_jobs: view_jobs::ViewJobs,
}

impl Browser {
    pub fn new(state: AppState, provider: Arc<dyn ProviderOps>, wake: Waker) -> Self {
        #[cfg(not(test))]
        let pools = Pools::shared();
        #[cfg(test)]
        let pools = Box::leak(Box::new(Pools::new()));
        Self::with_pools(state, provider, wake, pools)
    }

    #[expect(clippy::too_many_lines, reason = "one field per browser part")]
    fn with_pools(
        state: AppState,
        provider: Arc<dyn ProviderOps>,
        wake: Waker,
        pools: &'static Pools,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            state,
            provider,
            pools,
            owner: pools.queue(JobKind::Listing).owner(),
            size_owners: HashMap::new(),
            finds: HashMap::new(),
            find_walk: find::walker(),
            sizes: measure::SizeView::default(),
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
            job_queue: operate::queue::JobQueue::default(),
            ended: Vec::new(),
            filters: HashMap::new(),
            hidden_marks: HashMap::new(),
            full: HashMap::new(),
            shows: filter::Visibility::ALL,
            measures_folders: true,
            folder_totals: no_folder_totals(),
            default_sort: crate::file_list::SortKey::default(),
            folders_first: true,
            copying: operate::Copying::default(),
            sorts: HashMap::new(),
            sort_count: sort::SortCount::default(),
            next_op: 0,
            launcher: Arc::new(|_| {}),
            seen: Vec::new(),
            shown: HashMap::new(),
            cache: ListingCache::default(),
            keys: CacheKeys::default(),
            cached: HashMap::new(),
            pins: HashSet::new(),
            sorted: HashMap::new(),
            watching: watched::Watching::default(),
            profiles: ProfileCache::default(),
            stalls: offline::Stalls::default(),
            prefetches: prefetch::Prefetches::default(),
            hooks: retained::HookSlots::default(),
            view_jobs: view_jobs::ViewJobs::default(),
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
        let before = crate::perf::is_recording().then(|| self.shown_folder());
        let mut effects = self.apply_keeping_hidden_marks(command)?;
        effects.extend(open::save_request(command));
        self.drop_stale_filters();
        self.run(effects);
        if self.state.focused_pane() != focused {
            self.revalidate_focused();
        }
        if let Some(before) = before {
            self.note_navigation(&before);
        }
        Ok(())
    }

    fn note_navigation(&self, (tab, location): &(TabId, Location)) {
        let shown = self.state.active_tab();
        if shown.id() != *tab {
            crate::perf::cancel(crate::perf::Latency::FolderOpen);
        } else if shown.location() != location {
            crate::perf::begin(crate::perf::Latency::FolderOpen);
            self.note_folder_shown();
        }
    }

    fn shown_folder(&self) -> (TabId, Location) {
        let tab = self.state.active_tab();
        (tab.id(), tab.location().clone())
    }

    fn note_folder_shown(&self) {
        if self.state.active_tab().pending_request().is_none() {
            crate::perf::ready(crate::perf::Latency::FolderOpen);
        }
    }

    pub fn event(&mut self, event: Event) {
        let effects = apply_event(&mut self.state, event);
        self.run(effects);
    }

    #[must_use]
    pub fn profiles(&self) -> &ProfileCache {
        &self.profiles
    }

    #[must_use]
    pub fn loading(&self) -> bool {
        self.in_flight > 0 || self.measuring() || self.finds.values().any(FindResults::running)
    }

    #[must_use]
    pub fn state(&self) -> &AppState {
        &self.state
    }

    #[must_use]
    pub fn target_pane(&self) -> Option<PaneId> {
        self.state.window(self.state.focused_window())?.target()
    }

    pub fn remember_placement(&mut self, setup: &MonitorSetup, placement: Placement) {
        self.state.remember_placement(setup, placement);
    }

    #[must_use]
    pub fn rows(&self, tab: TabId) -> Rows {
        self.details
            .get(&tab)
            .filter(|_| self.is_loaded(tab))
            .cloned()
            .unwrap_or_default()
    }

    #[must_use]
    pub fn details(&self, tab: TabId) -> Arc<[FsEntry]> {
        self.rows(tab).to_arc()
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
        self.retain_size_owners();
        self.retain_finds();
        for effect in effects {
            match effect {
                Effect::ListFolder {
                    tab,
                    location,
                    request,
                } => self.list(tab, location, request),
                Effect::SaveSession => self.save_requested = true,
                Effect::MeasureFolder { tab, name, .. } if self.measures_folders => {
                    self.measure(tab, name);
                }
                Effect::MeasureFolder { .. } => {}
            }
        }
        self.sync_pins();
        self.sync_disk_folders();
        self.sync_watches();
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
        self.pools.queue(JobKind::Listing).submit(
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
                if self.shown.insert(tab, location.clone()).as_ref() != Some(&location) {
                    self.seen.push(open::Seen::Folder(location.clone()));
                }
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
        self.remark_hidden(tab);
        self.cached.remove(&tab);
        self.reshow_sizes(tab);
        self.plan_shown_sizes(tab);
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
mod compare;
#[cfg(test)]
pub(crate) use cached::tests::gated::Gated as GatedFs;
mod filter;
mod find;
mod measure;
mod message;
mod naming;
mod offline;
mod open;
mod operate;
mod prefetch;
mod rename_batch;
mod retained;
mod shown_sizes;
mod size_rows;
mod sort;
#[cfg(test)]
mod tests;
mod view_jobs;
mod watched;
