//! Background find run for the results window: one job walks every root in order.

use std::ops::ControlFlow;
use std::sync::mpsc::{self, Receiver, SyncSender};

use navo_core::Location;
use navo_core::find::FindQuery;
use navo_fs::find::{FindError, FindProgress, FoundBatch, search_root};
use navo_fs::{CancelToken, JobKind, JobSlot, OwnerGuard, Pools, Priority, volume_of};

use crate::command_center::content_source::ContentWalk;

/// Events buffered between the walk and the window before the walk waits.
const CHANNEL: usize = 8;

/// What a run reports back to the results window.
///
/// `root` indexes the roots `start` returns; a root that never resolved is numbered after them.
#[derive(Debug)]
pub enum RunEvent {
    Batch { root: u32, batch: FoundBatch },
    RootFailed { root: u32, error: String },
    Done(Result<FindProgress, String>),
}

/// A running search; dropping it cancels the walk.
pub struct FindRun {
    pub receive: Receiver<RunEvent>,
    cancel: CancelToken,
    _owner: OwnerGuard,
}

impl Drop for FindRun {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
impl FindRun {
    /// Builds a run directly around a test receiver without spawning a background job.
    pub(crate) fn from_receiver(receive: Receiver<RunEvent>) -> Self {
        let owner = Pools::shared().queue(JobKind::Find).owner();
        Self {
            receive,
            cancel: CancelToken::new(),
            _owner: owner,
        }
    }
}

/// Resolves the roots and starts one find job that walks them in order.
///
/// Returns the run and the resolved roots. Each walk sees the query with `within` cleared, so
/// its batches stay relative to their root. With a single root, its failure is the run's error.
pub fn start(walk: ContentWalk, base: Location, query: FindQuery) -> (FindRun, Vec<Location>) {
    let (roots, failed) = resolve(&base, &query);
    let queue = Pools::shared().queue(JobKind::Find);
    let owner = queue.owner();
    let slot = JobSlot {
        volume: volume_of(roots.first().unwrap_or(&base)),
        priority: Priority::Visible,
        owner: owner.id(),
    };
    let (send, receive) = mpsc::sync_channel(CHANNEL);
    let cancel = CancelToken::new();
    let mut query = query;
    query.within.clear();
    let job = RunJob {
        walk,
        roots: roots.clone(),
        failed,
        query,
        cancel: cancel.clone(),
        send,
    };
    queue.submit(slot, Box::new(move |queue_cancel| job.run(queue_cancel)));
    let run = FindRun {
        receive,
        cancel,
        _owner: owner,
    };
    (run, roots)
}

/// Splits `within` (or the base alone) into resolved roots and resolution errors.
fn resolve(base: &Location, query: &FindQuery) -> (Vec<Location>, Vec<String>) {
    let resolved: Vec<_> = if query.within.is_empty() {
        vec![search_root(base, None)]
    } else {
        query
            .within
            .iter()
            .map(|within| search_root(base, Some(within)))
            .collect()
    };
    let mut roots = Vec::new();
    let mut failed = Vec::new();
    for result in resolved {
        match result {
            Ok(root) => roots.push(root),
            Err(error) => failed.push(error.to_string()),
        }
    }
    (roots, failed)
}

/// The job's state: walks each root and reports through the bounded channel.
struct RunJob {
    walk: ContentWalk,
    roots: Vec<Location>,
    failed: Vec<String>,
    query: FindQuery,
    cancel: CancelToken,
    send: SyncSender<RunEvent>,
}

impl RunJob {
    fn run(self, queue_cancel: &CancelToken) {
        let single = self.roots.len() + self.failed.len() == 1;
        if single && let Some(error) = self.failed.first() {
            self.emit(RunEvent::Done(Err(error.clone())));
            return;
        }
        for (offset, error) in self.failed.iter().enumerate() {
            let root = index(self.roots.len() + offset);
            self.emit(RunEvent::RootFailed {
                root,
                error: error.clone(),
            });
        }
        let mut total = FindProgress::default();
        for (position, root) in self.roots.iter().enumerate() {
            let root_index = index(position);
            match self.walk_root(root, root_index, queue_cancel) {
                Ok(progress) => add(&mut total, progress),
                Err(error) if single || matches!(error, FindError::Cancelled) => {
                    self.emit(RunEvent::Done(Err(error.to_string())));
                    return;
                }
                Err(error) => self.emit(RunEvent::RootFailed {
                    root: root_index,
                    error: error.to_string(),
                }),
            }
            if self.stopped(queue_cancel) {
                self.emit(RunEvent::Done(Err(FindError::Cancelled.to_string())));
                return;
            }
        }
        self.emit(RunEvent::Done(Ok(total)));
    }

    fn walk_root(
        &self,
        root: &Location,
        root_index: u32,
        queue_cancel: &CancelToken,
    ) -> Result<FindProgress, FindError> {
        (self.walk)(root, &self.query, &self.cancel, &mut |batch| {
            if self.stopped(queue_cancel) {
                return ControlFlow::Break(());
            }
            let event = RunEvent::Batch {
                root: root_index,
                batch,
            };
            if self.send.send(event).is_err() {
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        })
    }

    fn stopped(&self, queue_cancel: &CancelToken) -> bool {
        self.cancel.is_cancelled() || queue_cancel.is_cancelled()
    }

    /// Sends an event, waiting for room; a dropped run makes this a no-op.
    fn emit(&self, event: RunEvent) {
        let _ = self.send.send(event);
    }
}

/// Adds one root's counts to the run total; the latest current file wins.
fn add(total: &mut FindProgress, progress: FindProgress) {
    total.scanned = total.scanned.saturating_add(progress.scanned);
    total.hits = total.hits.saturating_add(progress.hits);
    total.matched = total.matched.saturating_add(progress.matched);
    total.skipped = total.skipped.saturating_add(progress.skipped);
    if progress.current.is_some() {
        total.current = progress.current;
    }
}

fn index(position: usize) -> u32 {
    u32::try_from(position).unwrap_or(u32::MAX)
}
