use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use crate::model::{Budget, CancelToken, DiffStep, Reason, SideEntry, SideSource};

mod budget;
mod chunk;
mod content;
mod offset;
#[cfg(test)]
#[path = "tests/retained.rs"]
mod retained;

const EVENT_BUFFER: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Offset,
    Content,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteRegion {
    pub left: Range<u64>,
    pub right: Range<u64>,
    pub regions: u64,
    /// Affected span (the larger side), summed when merged; not an exact changed-byte count.
    pub changed: u64,
}

impl ByteRegion {
    pub(super) fn single(left: Range<u64>, right: Range<u64>) -> Self {
        let changed = left
            .end
            .saturating_sub(left.start)
            .max(right.end.saturating_sub(right.start));
        Self {
            left,
            right,
            regions: 1,
            changed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ByteEvent {
    Region(ByteRegion),
    Step(DiffStep),
    Progress { done: u64, total: u64 },
    Done,
    Failed(Reason),
}

#[derive(Clone)]
pub struct ByteSide {
    pub source: Arc<dyn SideSource>,
    pub entry: SideEntry,
}

impl ByteSide {
    #[must_use]
    pub fn new(source: Arc<dyn SideSource>, entry: SideEntry) -> Self {
        Self { source, entry }
    }
}

pub struct Request {
    pub left: ByteSide,
    pub right: ByteSide,
    pub align: Align,
    pub budget: Budget,
}

pub struct ByteDiff {
    events: Receiver<ByteEvent>,
    cancel: CancelToken,
    worker: Option<std::thread::JoinHandle<()>>,
    #[cfg(test)]
    retained: Arc<retained::Capacity>,
}

impl ByteDiff {
    #[must_use]
    pub fn start(request: Request, cancel: &CancelToken) -> Self {
        let (events, receive) = mpsc::sync_channel(EVENT_BUFFER);
        let fallback = events.clone();
        let cancel = cancel.clone();
        let worker_cancel = cancel.clone();
        #[cfg(test)]
        let retained = Arc::new(retained::Capacity::default());
        #[cfg(test)]
        let worker_retained = Arc::clone(&retained);
        let spawned = std::thread::Builder::new()
            .name("byte-diff".to_owned())
            .spawn(move || {
                #[cfg(test)]
                retained::install(worker_retained);
                run_worker(offset::Job {
                    left: request.left,
                    right: request.right,
                    align: request.align,
                    budget: request.budget,
                    cancel: &worker_cancel,
                    events: &events,
                });
            });
        let worker = match spawned {
            Ok(worker) => Some(worker),
            Err(_) => {
                offset::post(
                    &fallback,
                    &cancel,
                    ByteEvent::Failed(Reason::WorkerPanicked),
                );
                None
            }
        };
        Self {
            events: receive,
            cancel,
            worker,
            #[cfg(test)]
            retained,
        }
    }

    #[must_use]
    pub fn events(&self) -> &Receiver<ByteEvent> {
        &self.events
    }

    /// Cancel and wait for the engine worker; call off the UI thread.
    pub fn join(mut self) {
        self.cancel.cancel();
        if let Some(worker) = self.worker.take() {
            let _joined = worker.join();
        }
    }

    #[cfg(test)]
    fn retained_capacity(&self) -> (u64, u64) {
        self.retained.peak()
    }
}

fn run_worker(job: offset::Job<'_>) {
    let events = job.events;
    let cancel = job.cancel;
    if catch_unwind(AssertUnwindSafe(|| offset::run(job))).is_err() {
        offset::post(events, cancel, ByteEvent::Failed(Reason::WorkerPanicked));
    }
}

impl Drop for ByteDiff {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
