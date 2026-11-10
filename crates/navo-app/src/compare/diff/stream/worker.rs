//! Card workers: load one Stream item, build its card and hand it back to the session.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use navo_compare::model::CancelToken;
use navo_compare::text::{CardLimits, card as card_plan};
use navo_ui::compare_diff::{Card, PairRequest, StreamUpdate};

use super::Shared;
use super::search;
use super::state::{CardJob, Job, ViewedWrite};

use super::super::convert::to_card;
use super::super::load::load_pair;

/// Wait for a job, build its card and repeat until the session stops.
pub(super) fn run(shared: &Arc<Shared>) {
    loop {
        let Some(job) = wait(shared) else {
            return;
        };
        match job {
            Job::Card(job) => build(shared, job),
            Job::Viewed(write) => write_viewed(shared, write),
            Job::Search(job) => search::run(shared, job),
        }
    }
}

/// Take the next job, waiting on the queue until one arrives or the session stops.
fn wait(shared: &Arc<Shared>) -> Option<Job> {
    let mut state = shared.lock_state();
    loop {
        if shared.stopped.load(Ordering::Acquire) {
            return None;
        }
        if let Some(job) = state.take_job() {
            return Some(job);
        }
        while !state.has_work() && !shared.stopped.load(Ordering::Acquire) {
            state = shared
                .ready
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        if shared.stopped.load(Ordering::Acquire) {
            return None;
        }
    }
}

/// Load one job's pair, build its card and publish the result.
fn build(shared: &Arc<Shared>, job: CardJob) {
    let Some(card) = build_card(shared, &job.request, &job.cancel) else {
        shared.lock_state().abandon(job.id);
        return;
    };
    if job.cancel.is_cancelled() || shared.stopped.load(Ordering::Acquire) {
        shared.lock_state().abandon(job.id);
        return;
    }
    shared.store_and_publish(job.id, job.generation, card);
}

/// Load one pair and build its card; `None` when the load gives no pair or is cancelled.
pub(super) fn build_card(
    shared: &Arc<Shared>,
    request: &PairRequest,
    cancel: &CancelToken,
) -> Option<Arc<Card>> {
    let input = (shared.load)(request)?;
    let loaded = load_pair(&input, cancel);
    if cancel.is_cancelled() {
        return None;
    }
    let plan = loaded
        .alignment
        .as_ref()
        .map(|alignment| card_plan(alignment, CardLimits::default()))
        .unwrap_or_default();
    let mut card = to_card(&loaded, &plan);
    card.viewed = card.viewable
        && match shared.viewed.get(&card.viewed_key.0) {
            Ok(viewed) => viewed,
            Err(error) => {
                shared.warn_viewed(&error);
                false
            }
        };
    Some(Arc::new(card))
}

/// Store one Viewed mark and answer the UI.
fn write_viewed(shared: &Arc<Shared>, write: ViewedWrite) {
    if let Err(error) = shared.viewed.set(&write.key, write.viewed) {
        shared.warn_viewed(&error);
    }
    (shared.sink)(StreamUpdate::Viewed {
        id: write.id,
        viewed: write.viewed,
    });
}
