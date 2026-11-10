//! `DiffHooks` over the compare engine: one file pair per `Location` (D24, V12).
//!
//! The hooks only start workers: sides and the Viewed store are opened on those workers.

use std::sync::Arc;

use navo_compare::model::Reason;
use navo_compare::text::Options;
use navo_core::Location;
use navo_store::{CacheDb, StoreDirs};
use navo_ui::compare_diff::{
    DiffHooks, DiffSink, PairControl, PairRequest, StreamControl, StreamSink,
};
use navo_ui::settings::Values;

use super::super::settings::CompareSettings;
use super::load::{LoadInput, Side};
use super::pair::PairDiff;
use super::sides::{side_of, unreadable_side};
use super::stream::{LoadFn, StreamSession};
use super::viewed::{LazyViewed, NoViewed, OpenViewed, ViewedMarks, ViewedStore};
use crate::compare::convert::settings_of;

/// Builds one side from its `Location`; it touches the file system, so only workers call it.
pub(crate) type SideFn =
    Arc<dyn Fn(&Location, &CompareSettings) -> Result<Side, Reason> + Send + Sync>;

/// The app's `DiffHooks`: builds local and Git file sides and owns the Viewed store.
pub(crate) struct DiffBackend {
    side: SideFn,
    viewed: Arc<LazyViewed>,
}

impl Default for DiffBackend {
    /// Real local and Git sides and the profile cache, both opened later on workers.
    fn default() -> Self {
        Self::with_parts(Arc::new(side_of), Arc::new(open_viewed))
    }
}

impl DiffBackend {
    /// A backend over this side builder and Viewed opener; neither is called here.
    pub(crate) fn with_parts(side: SideFn, open: OpenViewed) -> Self {
        Self {
            side,
            viewed: Arc::new(LazyViewed::new(open)),
        }
    }
}

impl DiffHooks for DiffBackend {
    /// Start one pair's worker; it builds the sides and reports `Opened`.
    fn open_pair(
        &self,
        request: PairRequest,
        values: &Values,
        sink: DiffSink,
    ) -> Box<dyn PairControl> {
        let settings = settings_of(values);
        let side = Arc::clone(&self.side);
        let pair = request.clone();
        let opener = Box::new(move || input_of(&pair, &settings, &side).map(Arc::new));
        Box::new(PairDiff::open(request, opener, sink))
    }

    /// Open a Stream over the compared folder with a quarter-budget, at least 64 MiB.
    fn open_stream(&self, values: &Values, sink: StreamSink) -> Box<dyn StreamControl> {
        let settings = settings_of(values);
        let budget = (settings.budget().bytes() / 4).max(64 * 1024 * 1024);
        let side = Arc::clone(&self.side);
        let load: LoadFn =
            Arc::new(move |request| Some(input_or_unreadable(request, &settings, &side)));
        let viewed: Arc<dyn ViewedMarks> = self.viewed.clone();
        Box::new(StreamSession::start(sink, budget, load, viewed))
    }
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
