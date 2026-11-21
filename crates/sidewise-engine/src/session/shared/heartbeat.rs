//! The reading heartbeat: tracks content reads in flight, so the receiver hears from the
//! session at least every 250 ms while a slow file is being read, and learns which file it is.

use std::sync::{Arc, Condvar, PoisonError};
use std::time::{Duration, Instant};

use crate::model::{CancelToken, Generation, RelPath};
use crate::session::SlowEntry;

use super::{Shared, send};

/// Longest quiet gap the receiver sees while a content read runs.
const BEAT: Duration = Duration::from_millis(250);

/// The content reads in flight, oldest first, and the time of the last batch sent.
pub(super) struct Beat {
    reads: Vec<Read>,
    last_sent: Instant,
    /// Wakes the heartbeat thread; shared through an `Arc` so it can wait on the state lock.
    wake: Arc<Condvar>,
}

/// One running content read: the job id, the left path and when it started.
struct Read {
    id: u64,
    rel: RelPath,
    started: Instant,
}

impl Beat {
    /// An empty tracker that counts the quiet gap from now.
    pub(super) fn new() -> Self {
        Self {
            reads: Vec::new(),
            last_sent: Instant::now(),
            wake: Arc::new(Condvar::new()),
        }
    }

    /// Forgets every read; used when a new generation starts.
    pub(super) fn clear(&mut self) {
        self.reads.clear();
    }

    /// Notes a batch going out and returns the oldest running read with its age (ms).
    /// A final batch also wakes the heartbeat thread so it can exit.
    pub(super) fn on_send(&mut self, is_final: bool) -> Option<SlowEntry> {
        self.last_sent = Instant::now();
        if is_final {
            self.wake.notify_all();
        }
        self.reads.first().map(|read| SlowEntry {
            rel: read.rel.clone(),
            elapsed_ms: u64::try_from(read.started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    }

    /// Wakes the heartbeat thread so it re-checks cancel and generation.
    pub(super) fn wake(&self) {
        self.wake.notify_all();
    }
}

/// Ends its content read when dropped, so a panicking check never leaves a stale entry.
pub(in crate::session) struct ReadGuard<'a> {
    shared: &'a Shared,
    id: u64,
}

impl Drop for ReadGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.shared.state();
        if let Some(index) = state.beat.reads.iter().position(|read| read.id == self.id) {
            state.beat.reads.remove(index);
        }
    }
}

impl Shared {
    /// Records that job `id` started reading `rel`; the read ends when the guard drops.
    pub(in crate::session) fn begin_read(&self, id: u64, rel: RelPath) -> ReadGuard<'_> {
        let mut state = self.state();
        let was_idle = state.beat.reads.is_empty();
        state.beat.reads.push(Read {
            id,
            rel,
            started: Instant::now(),
        });
        if was_idle {
            state.beat.wake.notify_all();
        }
        ReadGuard { shared: self, id }
    }

    /// Runs on its own thread: sends an empty batch whenever a read runs and nothing was sent
    /// for 250 ms. Sleeps without a timer while no read runs; returns on the final batch,
    /// a closed session, a new generation or `cancel`.
    pub(in crate::session) fn beat_until_final(
        &self,
        generation: Generation,
        cancel: &CancelToken,
    ) {
        let mut state = self.state();
        loop {
            if cancel.is_cancelled()
                || state.closed
                || state.final_emitted
                || state.generation != generation
            {
                return;
            }
            let wake = Arc::clone(&state.beat.wake);
            if state.beat.reads.is_empty() {
                state = wake.wait(state).unwrap_or_else(PoisonError::into_inner);
                continue;
            }
            let left = BEAT.saturating_sub(state.beat.last_sent.elapsed());
            if left.is_zero() && !send(&mut state, Vec::new(), false, None) {
                return;
            }
            if !left.is_zero() {
                state = wake
                    .wait_timeout(state, left)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
            }
        }
    }
}
