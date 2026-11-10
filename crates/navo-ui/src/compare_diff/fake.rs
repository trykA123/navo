//! A fake `DiffHooks` for tests: it records every call and keeps the sinks so a test can push
//! updates as the app would.

use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use super::{
    CopyChange, CopySink, DiffHooks, DiffSink, PairControl, PairRequest, SheetPlanSink, SheetSink,
    StreamControl, StreamSink, ViewedKey,
};
use crate::settings::Values;

/// What the fake recorded.
#[derive(Default)]
pub(crate) struct Calls {
    pub(crate) pair_sink: Mutex<Option<DiffSink>>,
    pub(crate) stream_sink: Mutex<Option<StreamSink>>,
    pub(crate) requests: Mutex<Vec<PairRequest>>,
    pub(crate) spans: Mutex<Vec<Range<u32>>>,
    pub(crate) items: Mutex<Vec<(u64, usize)>>,
    pub(crate) searches: Mutex<Vec<(u64, String)>>,
    pub(crate) focused: Mutex<Vec<Vec<u64>>>,
    pub(crate) viewed: Mutex<Vec<(u64, bool)>>,
    pub(crate) acks: Mutex<Vec<u64>>,
    pub(crate) dropped: AtomicBool,
    pub(crate) copies: Mutex<Vec<CopyChange>>,
    pub(crate) copy_sink: Mutex<Option<CopySink>>,
    pub(crate) refreshed: Mutex<u32>,
    pub(crate) reloaded: Mutex<Vec<u64>>,
    pub(crate) sheets: Mutex<Vec<Vec<CopyChange>>>,
    pub(crate) sheet_plan_sink: Mutex<Option<SheetPlanSink>>,
    pub(crate) sheet_runs: Mutex<u32>,
    pub(crate) sheet_sink: Mutex<Option<SheetSink>>,
}

/// A fake that records calls and lets a test push updates.
#[derive(Default)]
pub(crate) struct FakeDiffHooks {
    pub(crate) calls: Arc<Calls>,
}

impl FakeDiffHooks {
    /// The sink of the last opened pair; panics when none was opened.
    pub(crate) fn pair_sink(&self) -> DiffSink {
        self.calls
            .pair_sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .expect("a pair was opened")
    }

    /// Every pair request so far, oldest first.
    pub(crate) fn requests(&self) -> Vec<PairRequest> {
        self.calls
            .requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The sink of the last opened Stream; panics when none was opened.
    pub(crate) fn stream_sink(&self) -> StreamSink {
        self.calls
            .stream_sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .expect("a Stream was opened")
    }

    /// The ids of the last `focus` call, if any.
    pub(crate) fn last_focus(&self) -> Option<Vec<u64>> {
        self.calls
            .focused
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .last()
            .cloned()
    }

    /// Every `set_viewed` call so far as (id, viewed).
    pub(crate) fn viewed(&self) -> Vec<(u64, bool)> {
        self.calls
            .viewed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Every `copy_change` request so far, oldest first.
    pub(crate) fn copies(&self) -> Vec<CopyChange> {
        self.calls
            .copies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The sink of the last copy; panics when none was asked.
    pub(crate) fn copy_sink(&self) -> CopySink {
        self.calls
            .copy_sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .expect("a copy was asked")
    }

    /// Every `plan_sheet` call so far: the changes each asked for.
    pub(crate) fn sheets(&self) -> Vec<Vec<CopyChange>> {
        self.calls
            .sheets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The sink of the last plan; panics when none was asked.
    pub(crate) fn sheet_plan_sink(&self) -> SheetPlanSink {
        self.calls
            .sheet_plan_sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .expect("a sheet was planned")
    }

    /// How many times `run_sheet` was called.
    pub(crate) fn sheet_runs(&self) -> u32 {
        *self
            .calls
            .sheet_runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The sink of the last sheet run; panics when none was asked.
    pub(crate) fn sheet_sink(&self) -> SheetSink {
        self.calls
            .sheet_sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .expect("a sheet was run")
    }

    /// The item ids the Stream was asked to reload, oldest first.
    pub(crate) fn reloaded(&self) -> Vec<u64> {
        self.calls
            .reloaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// How many times an open pair was asked to reload.
    pub(crate) fn refreshed(&self) -> u32 {
        *self
            .calls
            .refreshed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Every acknowledgement a pair control received, oldest first.
    pub(crate) fn acks(&self) -> Vec<u64> {
        self.calls
            .acks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Whether a pair control was dropped.
    pub(crate) fn dropped(&self) -> bool {
        self.calls.dropped.load(Ordering::SeqCst)
    }
}

struct FakePair(Arc<Calls>);
struct FakeStream(Arc<Calls>);

impl PairControl for FakePair {
    fn spans(&self, rows: Range<u32>) {
        self.0
            .spans
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(rows);
    }
    fn refresh(&self) {
        *self
            .0
            .refreshed
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += 1;
    }
    fn acknowledge(&self, upto: u64) {
        self.0
            .acks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(upto);
    }
}

impl Drop for FakePair {
    fn drop(&mut self) {
        self.0.dropped.store(true, Ordering::SeqCst);
    }
}

impl StreamControl for FakeStream {
    fn set_items(&self, generation: u64, items: Vec<(PairRequest, bool)>) {
        self.0
            .items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((generation, items.len()));
    }
    fn focus(&self, ids: Vec<u64>) {
        self.0
            .focused
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(ids);
    }
    fn set_viewed(&self, id: u64, _key: ViewedKey, viewed: bool) {
        self.0
            .viewed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((id, viewed));
    }
    fn search(&self, query: u64, text: String) {
        self.0
            .searches
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((query, text));
    }
    fn reload(&self, id: u64) {
        self.0
            .reloaded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(id);
    }
}

impl DiffHooks for FakeDiffHooks {
    fn open_pair(
        &self,
        request: PairRequest,
        _values: &Values,
        sink: DiffSink,
    ) -> Box<dyn PairControl> {
        self.calls
            .requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request);
        *self
            .calls
            .pair_sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(sink);
        Box::new(FakePair(self.calls.clone()))
    }

    fn copy_change(&self, change: CopyChange, done: CopySink) {
        self.copy_changes(vec![change], done);
    }

    fn copy_changes(&self, changes: Vec<CopyChange>, done: CopySink) {
        self.calls
            .copies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(changes);
        *self
            .calls
            .copy_sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(done);
    }

    fn plan_sheet(&self, changes: Vec<CopyChange>, planned: SheetPlanSink) {
        self.calls
            .sheets
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(changes);
        *self
            .calls
            .sheet_plan_sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(planned);
    }

    fn run_sheet(&self, done: SheetSink) {
        *self
            .calls
            .sheet_runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += 1;
        *self
            .calls
            .sheet_sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(done);
    }

    fn open_stream(&self, _values: &Values, sink: StreamSink) -> Box<dyn StreamControl> {
        *self
            .calls
            .stream_sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(sink);
        Box::new(FakeStream(self.calls.clone()))
    }
}
