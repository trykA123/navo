//! A fake `DiffHooks` for tests: it records every call and keeps the sinks so a test can push
//! updates as the app would.

use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use super::{DiffHooks, DiffSink, PairControl, PairRequest, StreamControl, StreamSink, ViewedKey};
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
    pub(crate) dropped: AtomicBool,
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
    fn refresh(&self) {}
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

    fn open_stream(&self, _values: &Values, sink: StreamSink) -> Box<dyn StreamControl> {
        *self
            .calls
            .stream_sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(sink);
        Box::new(FakeStream(self.calls.clone()))
    }
}
