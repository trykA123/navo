//! The detail lane: one thread that runs the card detail of the most central focused large, binary or image pair (D26.1).

use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use navo_ui::compare_diff::{CardDetail, StreamUpdate};

use super::super::load::LoadInput;
use super::detail_state::DetailFinish;
use super::{DetailJob, Shared};

/// How often the detail lane wakes to look for a job when the session has none.
pub(super) const DETAIL_POLL: Duration = Duration::from_millis(50);

/// The least time between two non-final snapshots of one job, in milliseconds (D26.3).
pub(super) const SNAPSHOT_GAP: Duration = Duration::from_millis(250);

/// Runs one detail job: its pair is loaded, then it emits snapshots through the sink.
pub(crate) type DetailFn =
    Arc<dyn Fn(&DetailJob, &LoadInput, &dyn Fn(CardDetail, bool)) + Send + Sync>;

/// Run detail jobs one at a time until the session stops.
pub(super) fn run(shared: &Arc<Shared>, detail: &DetailFn) {
    loop {
        let mut state = shared.lock_state();
        if shared.stopped.load(Ordering::Acquire) {
            return;
        }
        let Some(job) = state.take_detail() else {
            let (state, _) = shared
                .ready
                .wait_timeout(state, DETAIL_POLL)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            drop(state);
            continue;
        };
        drop(state);
        let job_id = job.id;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_job(shared, detail, job);
        }));
        if result.is_err() {
            tracing::warn!(id = job_id, "detail job panicked");
        }
    }
}

/// Frees the single detail lane when the job ends, normally or by unwinding.
struct LaneGuard<'a> {
    shared: &'a Shared,
    job: &'a DetailJob,
}

impl Drop for LaneGuard<'_> {
    fn drop(&mut self) {
        self.shared.lock_state().end_detail(self.job);
    }
}

/// Load and run one job; the lane guard frees it even when the job panics.
fn run_job(shared: &Arc<Shared>, detail: &DetailFn, job: DetailJob) {
    let _lane = LaneGuard { shared, job: &job };
    if let Some(input) = (shared.load)(&job.request) {
        let published: Cell<Option<Instant>> = Cell::new(None);
        let emit = |value: CardDetail, last: bool| {
            if !last
                && published
                    .get()
                    .is_some_and(|at| at.elapsed() < SNAPSHOT_GAP)
            {
                return;
            }
            published.set(Some(Instant::now()));
            shared.store_detail_and_publish(&job, Arc::new(value), last);
        };
        detail(&job, &input, &emit);
    }
}

impl Shared {
    /// Store one detail snapshot and send it in event order, with the cards it evicted.
    pub(super) fn store_detail_and_publish(
        &self,
        job: &DetailJob,
        detail: Arc<CardDetail>,
        last: bool,
    ) {
        let mut state = self.lock_state();
        let Some(finish) = state.store_detail(job, detail, last) else {
            return;
        };
        let _order = self.lock_order();
        drop(state);
        let DetailFinish {
            generation,
            id,
            detail,
            evicted,
        } = finish;
        let new_evicted = evicted.contains(&id);
        for victim in evicted.into_iter().filter(|victim| *victim != id) {
            (self.sink)(StreamUpdate::Evicted {
                generation,
                id: victim,
            });
        }
        if new_evicted {
            (self.sink)(StreamUpdate::Evicted { generation, id });
        } else {
            (self.sink)(StreamUpdate::Detail {
                generation,
                id,
                detail,
            });
        }
    }
}
