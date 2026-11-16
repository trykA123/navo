//! `DiffHooks` over the compare engine: one file pair per `Location` (D24, V12).
//!
//! The hooks only start workers: sides and the Viewed store are opened on those workers.

use std::sync::Arc;

use navo_compare::model::Reason;
use navo_compare::text::Options;
use navo_core::Location;
use navo_fs::ProviderOps;
use navo_store::{CacheDb, StoreDirs};
use navo_ui::browser::LogFactory;
use navo_ui::compare_diff::{
    CompareOptions, CopyChange, CopySink, DiffHooks, DiffSink, PairControl, PairRequest, SheetPlan,
    SheetPlanSink, SheetResult, SheetSink, StreamControl, StreamSink,
};

use super::super::settings::CompareSettings;
use super::cards;
use super::copy::CopyRefusal;
use super::copy_hook::{CopyService, unavailable};
use super::load::{LoadInput, Side};
use super::navo_backend::navo_compare_dirs;
use super::pair::{EnginePlan, PairDiff};
use super::sides::{side_of, unreadable_side};
use super::sources::Sources;
use super::stream::{LoadFn, StreamParts, StreamSession};
use super::viewed::{LazyViewed, NoViewed, OpenViewed, ViewedMarks, ViewedStore};
use crate::compare::convert::settings_from;
use crate::compare::dirs::CompareDirs;

/// Builds one side from its `Location`; it touches the file system, so only workers call it.
pub(crate) type SideFn =
    Arc<dyn Fn(&Location, &CompareSettings) -> Result<Side, Reason> + Send + Sync>;

/// The app's `DiffHooks`: builds local and Git file sides and owns the Viewed store. Copying a
/// change needs the compare job factory; without one every copy is refused.
pub struct DiffBackend {
    dirs: CompareDirs,
    side: SideFn,
    viewed: Arc<LazyViewed>,
    copy: Option<CopyService>,
}

/// What a host passes to build a backend: its folders, its side builder and its Viewed opener.
/// Crate-visible until `SideFn` and `OpenViewed` are public (G4).
pub(crate) struct BackendParts {
    pub dirs: CompareDirs,
    pub side: SideFn,
    pub viewed: OpenViewed,
}

impl Default for DiffBackend {
    /// Real local and Git sides, the profile cache and Navo's detected spool.
    fn default() -> Self {
        Self::new(navo_parts(navo_compare_dirs(StoreDirs::detect().as_ref())))
    }
}

impl DiffBackend {
    /// A backend over `parts`; it sweeps dead spool folders in `parts.dirs.spool` only, and
    /// calls neither the side builder nor the Viewed opener here.
    pub(crate) fn new(parts: BackendParts) -> Self {
        Sources::sweep_startup(parts.dirs.spool.as_deref());
        Self {
            dirs: parts.dirs,
            side: parts.side,
            viewed: Arc::new(LazyViewed::new(parts.viewed)),
            copy: None,
        }
    }

    /// A backend over this side builder and Viewed opener, with no spool; for tests.
    pub(crate) fn with_parts(side: SideFn, open: OpenViewed) -> Self {
        Self::new(BackendParts {
            dirs: CompareDirs::default(),
            side,
            viewed: open,
        })
    }

    /// The default backend whose copies run through `factory` and write with `ops`.
    #[cfg(test)]
    pub(crate) fn with_copy(factory: LogFactory, ops: Arc<dyn ProviderOps>) -> Self {
        Self::default().copying(factory, ops)
    }

    /// This backend with its copies run through `factory`, writing with `ops`.
    pub(crate) fn copying(mut self, factory: LogFactory, ops: Arc<dyn ProviderOps>) -> Self {
        self.copy = Some(CopyService::new(factory, ops));
        self
    }

    /// The engine plan for one pair or Stream under `settings`, spooling in the host's root.
    pub(super) fn plan(&self, settings: CompareSettings) -> EnginePlan {
        EnginePlan::new(settings, self.dirs.spool.clone())
    }
}

/// Navo's sides and Viewed opener over `dirs`.
pub(super) fn navo_parts(dirs: CompareDirs) -> BackendParts {
    BackendParts {
        dirs,
        side: Arc::new(side_of),
        viewed: Arc::new(open_viewed),
    }
}

impl DiffHooks for DiffBackend {
    /// Start one pair's worker; it builds the sides and reports `Opened`.
    fn open_pair(
        &self,
        request: PairRequest,
        options: &CompareOptions,
        sink: DiffSink,
    ) -> Box<dyn PairControl> {
        let settings = settings_from(options);
        let plan = self.plan(settings.clone());
        let side = Arc::clone(&self.side);
        let pair = request.clone();
        let opener = Box::new(move || input_of(&pair, &settings, &side).map(Arc::new));
        Box::new(PairDiff::open(request, opener, plan, sink))
    }

    /// Open a Stream over the compared folder with a quarter-budget, at least 64 MiB.
    fn open_stream(&self, options: &CompareOptions, sink: StreamSink) -> Box<dyn StreamControl> {
        let settings = settings_from(options);
        let budget = (settings.budget().bytes() / 4).max(64 * 1024 * 1024);
        let side = Arc::clone(&self.side);
        let load_settings = settings.clone();
        let load: LoadFn =
            Arc::new(move |request| Some(input_or_unreadable(request, &load_settings, &side)));
        let viewed: Arc<dyn ViewedMarks> = self.viewed.clone();
        let detail = Some(cards::detail_fn(self.plan(settings)));
        Box::new(StreamSession::start_with(
            sink,
            budget,
            StreamParts {
                load,
                viewed,
                detail,
            },
        ))
    }

    /// Copy one change on a new thread, or refuse when the app has no job factory.
    fn copy_change(&self, change: CopyChange, done: CopySink) {
        match &self.copy {
            Some(service) => service.start(change, done),
            None => unavailable(&change, &done),
        }
    }

    /// Plan many changes on a new thread, or report an empty plan without a job factory.
    fn plan_sheet(&self, changes: Vec<CopyChange>, planned: SheetPlanSink) {
        match &self.copy {
            Some(service) => service.plan_sheet(changes, planned),
            None => planned(SheetPlan::default()),
        }
    }

    /// Run the last plan as one job on a new thread, or refuse without a job factory.
    fn run_sheet(&self, done: SheetSink) {
        match &self.copy {
            Some(service) => service.run_sheet(done),
            None => done(SheetResult::Refused(CopyRefusal::NotCopyable.text())),
        }
    }

    /// Copy several changes of one pair as one job, or refuse when there is no job factory.
    fn copy_changes(&self, changes: Vec<CopyChange>, done: CopySink) {
        match (&self.copy, changes.first()) {
            (Some(service), Some(_)) => service.start_many(changes, done),
            (None, Some(first)) => unavailable(first, &done),
            (_, None) => {}
        }
    }
}

/// The compare rules and fallback encoding every pair is read under. A copy matches the shown
/// change against a fresh diff, so it must use the same ones.
pub(super) fn rules() -> (Options, &'static encoding_rs::Encoding) {
    (Options::default(), encoding_rs::UTF_8)
}

/// Open the profile cache for Viewed marks, or fall back to memory and a no-op store.
fn open_viewed() -> Arc<dyn ViewedMarks> {
    let db = StoreDirs::detect()
        .filter(|dirs| dirs.ensure().is_ok())
        .and_then(|dirs| CacheDb::open(&dirs.data.join("cache.db")).ok())
        .map(|(db, _)| db);
    match db.or_else(|| CacheDb::open_in_memory().ok()) {
        Some(db) => Arc::new(ViewedStore::new(db)),
        None => Arc::new(NoViewed),
    }
}

/// Build one pair, reporting the first side that cannot open.
fn input_of(
    request: &PairRequest,
    settings: &CompareSettings,
    side: &SideFn,
) -> Result<LoadInput, Reason> {
    let left = match &request.left {
        Some(location) => Some(side(location, settings)?),
        None => None,
    };
    let right = match &request.right {
        Some(location) => Some(side(location, settings)?),
        None => None,
    };
    Ok(input(left, right))
}

/// Build a pair for a Stream card; a side that cannot open stays present but unreadable, so the
/// card reports it Unavailable and gets no Viewed key.
fn input_or_unreadable(
    request: &PairRequest,
    settings: &CompareSettings,
    side: &SideFn,
) -> LoadInput {
    let build = |location: &Location| {
        side(location, settings).unwrap_or_else(|reason| unreadable_side(location, reason))
    };
    input(
        request.left.as_ref().map(build),
        request.right.as_ref().map(build),
    )
}

/// The loader input for two optional sides.
fn input(left: Option<Side>, right: Option<Side>) -> LoadInput {
    LoadInput {
        left,
        right,
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    }
}
