//! What the fake provider recorded for tests: how often each operation ran, and the delays a
//! job waited before a retry (the fake clock records them instead of sleeping).

use std::time::{Duration, Instant};

use super::{FakeFs, FakeOp, lock, normalize};
use crate::CancelToken;

/// The fake clock: the waits so far, a token to cancel during the next wait, and its start.
#[derive(Debug, Default)]
pub(super) struct Clock {
    pub(super) waits: Vec<Duration>,
    pub(super) cancels: Option<CancelToken>,
    origin: Option<Instant>,
}

impl Clock {
    /// The fake time: the first reading plus every recorded wait, so only waits move it.
    pub(super) fn now(&mut self) -> Instant {
        *self.origin.get_or_insert_with(Instant::now) + self.waits.iter().sum::<Duration>()
    }
}

impl FakeFs {
    /// How many times `op` ran on `path` so far.
    #[must_use]
    pub fn calls(&self, path: &str, op: FakeOp) -> u32 {
        lock(&self.calls)
            .get(&(normalize(path).to_owned(), op))
            .copied()
            .unwrap_or(0)
    }

    /// Every delay a job waited before a retry on this provider, oldest first.
    #[must_use]
    pub fn retry_waits(&self) -> Vec<Duration> {
        lock(&self.clock).waits.clone()
    }

    /// Cancels `cancel` while the next retry wait runs, as a user pressing Cancel then would.
    pub fn cancel_during_next_wait(&self, cancel: CancelToken) {
        lock(&self.clock).cancels = Some(cancel);
    }
}
