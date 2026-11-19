//! Schedules at most one delayed verification build per volume.

use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct VerifyRebuild {
    pending: Option<Instant>,
    attempts: u32,
}

impl VerifyRebuild {
    /// Schedules verification after an inexact result and returns its deadline.
    pub(super) fn inexact(&mut self, now: Instant) -> Instant {
        let minutes = (10u64 << self.attempts).min(60);
        *self
            .pending
            .get_or_insert(now + Duration::from_secs(minutes * 60))
    }

    /// Clears verification state after an exact build.
    pub(super) fn exact(&mut self) {
        *self = Self::default();
    }

    /// Consumes a verification when its deadline has passed.
    pub(super) fn take_due(&mut self, now: Instant) -> bool {
        if self.pending.is_some_and(|deadline| now >= deadline) {
            self.pending = None;
            self.attempts = (self.attempts + 1).min(3);
            return true;
        }
        false
    }

    /// Reports whether a verification is waiting to run.
    pub(super) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    /// Verifies each retry interval through the cap.
    #[test]
    fn verification_delays_double_and_cap_at_sixty_minutes() {
        let mut verification = VerifyRebuild::default();
        let mut now = Instant::now();
        for minutes in [10, 20, 40, 60, 60] {
            let due = verification.inexact(now);
            assert_eq!(due.duration_since(now), minutes * MINUTE);
            assert!(verification.take_due(due));
            now = due;
        }
    }

    /// Clears both the next run and the retry escalation on success.
    #[test]
    fn an_exact_build_resets_verification() {
        let mut verification = VerifyRebuild::default();
        let now = Instant::now();
        let due = verification.inexact(now);
        assert!(verification.take_due(due));
        verification.inexact(due);
        verification.exact();
        assert!(
            !verification.take_due(due + 60 * MINUTE),
            "exact clears the pending run"
        );
        assert_eq!(verification.inexact(due), due + 10 * MINUTE);
    }

    /// Keeps the original deadline when more inexact results arrive while it is pending.
    #[test]
    fn only_one_verification_can_be_pending() {
        let mut verification = VerifyRebuild::default();
        let now = Instant::now();
        let due = verification.inexact(now);
        assert_eq!(
            verification.inexact(now + MINUTE).duration_since(now),
            10 * MINUTE
        );
        assert!(verification.take_due(due));
        assert!(!verification.take_due(due + MINUTE));
    }
}
