//! Chooses how long the index worker waits after a journal follow attempt.

use std::time::Duration;

#[derive(Default)]
pub(super) struct Pace {
    failures: u32,
}

pub enum Outcome {
    Moved,
    Idle,
    Failed,
}

impl Pace {
    /// Returns the delay after a follow attempt and resets backoff on success.
    pub fn next(&mut self, outcome: Outcome) -> Duration {
        match outcome {
            Outcome::Moved => {
                self.failures = 0;
                Duration::ZERO
            }
            Outcome::Idle => {
                self.failures = 0;
                Duration::from_millis(500)
            }
            Outcome::Failed => {
                self.failures = self.failures.saturating_add(1);
                Duration::from_secs((1u64 << self.failures.min(9)).min(300))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks capped backoff and the delay after progress or an idle poll.
    #[test]
    fn failures_back_off_until_a_successful_poll() {
        let mut pace = Pace::default();
        for seconds in [2, 4, 8, 16, 32, 64, 128, 256, 300, 300] {
            assert_eq!(pace.next(Outcome::Failed), Duration::from_secs(seconds));
        }

        assert_eq!(pace.next(Outcome::Moved), Duration::ZERO);
        assert_eq!(pace.next(Outcome::Failed), Duration::from_secs(2));
        assert_eq!(pace.next(Outcome::Idle), Duration::from_millis(500));
        assert_eq!(pace.next(Outcome::Failed), Duration::from_secs(2));
    }
}
