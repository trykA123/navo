//! What the scanner does when a file operation pauses a status: settle the proof, keep the
//! badge, and retry the root with a growing delay.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use navo_core::Location;

use super::listing::Discovered;
use super::plan::{Plan, Timer};
use super::queue::{Listing, Request, Shared};
use super::watches::root_key;
use super::worker::Done;

/// The first retry delay after a paused status.
pub(super) const PAUSE_RETRY: Duration = Duration::from_millis(500);

/// The longest retry delay.
const MAX_RETRY: Duration = Duration::from_secs(4);

/// The retry delay already tried for each paused (listing, root).
#[derive(Default)]
pub(super) struct Retries {
    delays: HashMap<(usize, PathBuf), Duration>,
}

impl Retries {
    /// The next delay for this pause: 500 ms, 1 s, 2 s, then 4 s at most.
    pub(super) fn next_delay(&mut self, listing: &Arc<Listing>, root: &Path) -> Duration {
        let key = (Arc::as_ptr(listing) as usize, root.to_path_buf());
        let delay = self
            .delays
            .get(&key)
            .map_or(PAUSE_RETRY, |last| (*last * 2).min(MAX_RETRY));
        self.delays.insert(key, delay);
        delay
    }

    /// Forgets the pause history after a result that is not Paused.
    pub(super) fn reset(&mut self, listing: &Arc<Listing>, root: &Path) {
        self.delays
            .remove(&(Arc::as_ptr(listing) as usize, root.to_path_buf()));
    }

    /// Drops history for listings the scanner no longer holds.
    pub(super) fn retain(&mut self, listings: &HashMap<Location, Discovered>) {
        self.delays.retain(|(ptr, _), _| {
            listings
                .values()
                .any(|found| Arc::as_ptr(&found.listing) as usize == *ptr)
        });
    }
}

/// Settles a paused status: releases the forced mark, drops the proof and schedules the retry.
pub(super) fn settle_paused(shared: &Mutex<Shared>, plan: &mut Plan, done: Done, now: Instant) {
    let Done {
        listing,
        root,
        check,
        result,
    } = done;
    {
        let mut shared = shared.lock().unwrap_or_else(PoisonError::into_inner);
        if shared.is_current(&listing) {
            shared.forced_done(&listing, &root_key(&root));
        }
    }
    plan.finish(&root, check, &result);
    let delay = plan.retries.next_delay(&listing, &root);
    plan.retry_at(&listing, &root, now + delay);
}

/// Queues one root's status again, or re-lists the folder when the listing was replaced.
pub(super) fn retry_now(shared: &Mutex<Shared>, listing: &Arc<Listing>, root: PathBuf) {
    let mut shared = shared.lock().unwrap_or_else(PoisonError::into_inner);
    if shared.is_current(listing) {
        shared.push(Request::Status(listing.clone(), root));
    } else {
        shared.rescan_repo(&listing.folder, root);
    }
}

/// Whether two timers name the same listing and root.
pub(super) fn same_place(a: &Timer, b: &Timer) -> bool {
    match (a, b) {
        (Timer::Confirm { listing: a, .. }, Timer::Confirm { listing: b, .. }) => Arc::ptr_eq(a, b),
        (
            Timer::Expire {
                listing: a,
                root: ar,
            },
            Timer::Expire {
                listing: b,
                root: br,
            },
        )
        | (
            Timer::Retry {
                listing: a,
                root: ar,
            },
            Timer::Retry {
                listing: b,
                root: br,
            },
        ) => Arc::ptr_eq(a, b) && ar == br,
        _ => false,
    }
}
