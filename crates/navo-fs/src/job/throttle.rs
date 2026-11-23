//! The job speed cap: a token bucket in bytes per second, off unless `JobOptions` sets a cap.
//! It waits inside a copy's progress report, so the plain, CopyFile2 and streamed copies all slow.

use std::num::NonZeroU64;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::copy::Copier;

/// How many bytes the cap lets through at once after an idle spell, as time at full rate.
const BURST: Duration = Duration::from_millis(100);

/// One bucket for the whole job, shared by its copy workers. It keeps the time at which the bytes
/// taken so far are paid off at the cap (the GCRA form of a token bucket: no floats, no refill timer).
#[derive(Debug)]
pub(super) struct Throttle {
    rate: NonZeroU64,
    paid_off: Mutex<Option<Instant>>,
}

impl Throttle {
    /// A bucket that lets `rate` bytes per second through, full at the start.
    pub(super) fn new(rate: NonZeroU64) -> Self {
        Self {
            rate,
            paid_off: Mutex::new(None),
        }
    }

    /// Takes `bytes` from the bucket at time `now`; returns how long the caller waits before it
    /// sends more (zero while the bucket still holds them).
    fn take(&self, bytes: u64, now: Instant) -> Duration {
        let nanos = u128::from(bytes) * 1_000_000_000 / u128::from(self.rate.get());
        let cost = Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX));
        // The bucket only holds an Instant, so a panic elsewhere cannot leave it half written.
        let mut paid_off = self.paid_off.lock().unwrap_or_else(PoisonError::into_inner);
        let full = now.checked_sub(BURST).unwrap_or(now);
        let start = paid_off.map_or(full, |at| at.max(full));
        let end = start.checked_add(cost).unwrap_or(start);
        *paid_off = Some(end);
        end.saturating_duration_since(now)
    }
}

/// Wraps a copy's `progress` report (bytes so far in this attempt) so each report also takes the
/// new bytes from the job's bucket and waits on the provider clock. A cancel wakes the wait; the
/// copy then stops at its next cancel check. Without a cap the report passes straight through.
pub(super) fn meter<'a>(
    copier: &'a Copier<'_>,
    progress: &'a mut dyn FnMut(u64),
) -> impl FnMut(u64) + 'a {
    let mut sent = 0;
    move |total: u64| {
        progress(total);
        let Some(throttle) = copier.throttle else {
            return;
        };
        let new = total.saturating_sub(sent);
        sent = sent.max(total);
        if new == 0 || copier.cancel.is_cancelled() {
            return;
        }
        let wait = throttle.take(new, copier.ops.now());
        if !wait.is_zero() {
            copier.ops.wait_to_retry(wait, copier.cancel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    #[test]
    fn a_full_bucket_lets_the_burst_through_then_charges_the_rate() {
        let throttle = Throttle::new(NonZeroU64::new(1_000_000).unwrap());
        let now = Instant::now() + Duration::from_secs(1);
        assert_eq!(throttle.take(100_000, now), Duration::ZERO);
        assert_eq!(throttle.take(500_000, now), Duration::from_millis(500));
    }

    #[test]
    fn an_idle_spell_refills_only_up_to_the_burst() {
        let throttle = Throttle::new(NonZeroU64::new(MIB).unwrap());
        let now = Instant::now() + Duration::from_secs(1);
        assert_eq!(throttle.take(MIB, now), Duration::from_millis(900));
        let later = now + Duration::from_secs(60);
        assert_eq!(throttle.take(MIB, later), Duration::from_millis(900));
    }
}
