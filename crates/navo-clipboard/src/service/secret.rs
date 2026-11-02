//! Secret plain text, the sealing port, the clock and the timer that empties the clipboard.

use std::fmt;
use std::time::{Duration, Instant};

use zeroize::Zeroize;

/// How long a copied secret stays on the clipboard.
pub const SECRET_CLEAR_AFTER: Duration = Duration::from_secs(30);

/// Plain secret bytes. The buffer, spare capacity included, is zeroed on drop; never cloned.
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The bytes as text, when they are valid UTF-8.
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for SecretBytes {
    fn from(text: String) -> Self {
        Self(text.into_bytes())
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        wipe(&mut self.0);
        #[cfg(test)]
        wipe_hook::record(&self.0);
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretBytes(len {})", self.0.len())
    }
}

/// Zeroes every byte the vector owns; it keeps its length so the result can be checked.
fn wipe(bytes: &mut Vec<u8>) {
    bytes.resize(bytes.capacity(), 0);
    bytes.as_mut_slice().zeroize();
}

/// Why sealing or unsealing failed. Never carries the bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SealError {
    #[error("secrets need Windows")]
    Unsupported,
    #[error("the secret could not be sealed or unsealed")]
    Failed,
}

/// Seals secret plain text for the store and opens it again. Called on the worker thread only.
pub trait Sealer: Send + Sync {
    fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, SealError>;
    fn unseal(&self, sealed: &[u8]) -> Result<SecretBytes, SealError>;
}

/// The sealer of builds without an OS sealer: refuses every call.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSealer;

impl Sealer for NoSealer {
    fn seal(&self, _plain: &[u8]) -> Result<Vec<u8>, SealError> {
        Err(SealError::Unsupported)
    }

    fn unseal(&self, _sealed: &[u8]) -> Result<SecretBytes, SealError> {
        Err(SealError::Unsupported)
    }
}

/// Monotonic time for the clear timer.
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// The clipboard change count a write produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sequence(pub u32);

/// How long a failed clear waits before the next attempt.
pub(super) const CLEAR_RETRY: Duration = Duration::from_secs(2);
/// Clear attempts before a secret copy is given up; stop and drop still try once more.
pub(super) const CLEAR_ATTEMPTS: u32 = 15;

struct Pending {
    sequence: Sequence,
    deadline: Option<Instant>,
    failures: u32,
}

/// What a clear attempt left behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Settled {
    Done,
    Retry,
    GaveUp,
}

/// The one secret copy that still has to be cleared. It stays until a clear succeeds or
/// finds a newer copy; after `CLEAR_ATTEMPTS` failures it waits only for stop.
#[derive(Default)]
pub(super) struct ClearTimer {
    pending: Option<Pending>,
}

impl ClearTimer {
    pub(super) fn start(&mut self, sequence: Sequence, now: Instant) {
        self.pending = Some(Pending {
            sequence,
            deadline: Some(now + SECRET_CLEAR_AFTER),
            failures: 0,
        });
    }

    /// How long until the next attempt; `None` when none is scheduled.
    pub(super) fn wait(&self, now: Instant) -> Option<Duration> {
        let deadline = self.pending.as_ref()?.deadline?;
        Some(deadline.saturating_duration_since(now))
    }

    /// Whether an attempt is scheduled at or before `now`.
    pub(super) fn is_due(&self, now: Instant) -> bool {
        self.pending
            .as_ref()
            .and_then(|pending| pending.deadline)
            .is_some_and(|deadline| deadline <= now)
    }

    pub(super) fn sequence(&self) -> Option<Sequence> {
        self.pending.as_ref().map(|pending| pending.sequence)
    }

    /// Records one attempt: `done` retires the copy, a failure schedules a retry or gives up.
    pub(super) fn settle(&mut self, done: bool, now: Instant) -> Settled {
        let Some(pending) = self.pending.as_mut() else {
            return Settled::Done;
        };
        if done {
            self.pending = None;
            return Settled::Done;
        }
        pending.failures = pending.failures.saturating_add(1);
        if pending.failures >= CLEAR_ATTEMPTS {
            pending.deadline = None;
            return Settled::GaveUp;
        }
        pending.deadline = Some(now + CLEAR_RETRY);
        Settled::Retry
    }

    pub(super) fn take(&mut self) -> Option<Sequence> {
        self.pending.take().map(|pending| pending.sequence)
    }
}

#[cfg(test)]
pub(super) mod wipe_hook {
    use std::cell::RefCell;

    thread_local! {
        static LAST: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
    }

    pub(in crate::service) fn record(bytes: &[u8]) {
        LAST.with(|last| *last.borrow_mut() = Some(bytes.to_vec()));
    }

    /// The buffer of the last `SecretBytes` dropped on this thread, as it was after the wipe.
    pub(in crate::service) fn take() -> Option<Vec<u8>> {
        LAST.with(|last| last.borrow_mut().take())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_zeroes_the_whole_buffer_spare_capacity_included() {
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(b"hunter2-canary");
        drop(SecretBytes::new(bytes));
        let wiped = wipe_hook::take().unwrap();
        assert_eq!(wiped.len(), 64);
        assert!(wiped.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn debug_shows_the_length_only() {
        let secret = SecretBytes::from("hunter2-canary".to_owned());
        let shown = format!("{secret:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(shown.contains("14"), "{shown}");
    }

    #[test]
    fn no_sealer_refuses_both_ways() {
        assert_eq!(NoSealer.seal(b"x"), Err(SealError::Unsupported));
        assert!(matches!(NoSealer.unseal(b"x"), Err(SealError::Unsupported)));
    }

    #[test]
    fn the_timer_is_due_only_at_its_deadline() {
        let start = Instant::now();
        let mut timer = ClearTimer::default();
        assert_eq!(timer.wait(start), None);
        timer.start(Sequence(7), start);
        assert_eq!(timer.wait(start), Some(SECRET_CLEAR_AFTER));
        assert!(!timer.is_due(start + SECRET_CLEAR_AFTER - Duration::from_millis(1)));
        assert!(timer.is_due(start + SECRET_CLEAR_AFTER));
        assert_eq!(timer.sequence(), Some(Sequence(7)));
        assert_eq!(timer.settle(true, start), Settled::Done);
        assert_eq!(timer.take(), None);
    }

    #[test]
    fn a_failed_clear_is_retried_until_it_gives_up_but_stays_for_stop() {
        let start = Instant::now();
        let mut timer = ClearTimer::default();
        timer.start(Sequence(7), start);
        for _ in 1..CLEAR_ATTEMPTS {
            assert_eq!(timer.settle(false, start), Settled::Retry);
            assert_eq!(timer.wait(start), Some(CLEAR_RETRY));
        }
        assert_eq!(timer.settle(false, start), Settled::GaveUp);
        assert_eq!(timer.wait(start), None);
        assert!(!timer.is_due(start + SECRET_CLEAR_AFTER * 10));
        assert_eq!(timer.take(), Some(Sequence(7)));
    }
}
