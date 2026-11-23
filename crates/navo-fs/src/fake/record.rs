//! What the fake provider recorded for tests: how often each operation ran, and the delays a
//! job waited before a retry (the fake clock records them instead of sleeping).

use std::time::Duration;

use super::{FakeFs, FakeOp, lock, normalize};
use crate::CancelToken;

/// The fake clock: the retry waits so far, and a token to cancel during the next wait.
#[derive(Debug, Default)]
pub(super) struct Clock {
    pub(super) waits: Vec<Duration>,
    pub(super) cancels: Option<CancelToken>,
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
