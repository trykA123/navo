//! One shared bound for user-started recursive walks on a network path (D103).
//!
//! Lives here so `navo-fs` and `navo-compare` apply the same rule.

use std::time::{Duration, Instant};

/// Entries a user-started walk may visit on a network path before it asks to continue.
pub const NETWORK_WALK_MAX_ENTRIES: u64 = 200_000;
/// Time a user-started walk may run on a network path before it asks to continue.
pub const NETWORK_WALK_MAX_TIME: Duration = Duration::from_secs(60);

/// Why a network walk stopped at its bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkStop {
    Entries,
    Time,
}

/// Counts entries and time for one network walk; `resume` starts the next bound after the user
/// chose to continue.
#[derive(Clone, Copy, Debug)]
pub struct NetworkWalkBudget {
    started: Instant,
    entries: u64,
    max_entries: u64,
    max_time: Duration,
}

impl NetworkWalkBudget {
    #[must_use]
    pub fn new(now: Instant) -> Self {
        Self::with_limits(now, NETWORK_WALK_MAX_ENTRIES, NETWORK_WALK_MAX_TIME)
    }

    #[must_use]
    pub fn with_limits(now: Instant, max_entries: u64, max_time: Duration) -> Self {
        Self {
            started: now,
            entries: 0,
            max_entries,
            max_time,
        }
    }

    /// Adds `visited` entries; `Some` once either bound is reached.
    pub fn visit(&mut self, visited: u64, now: Instant) -> Option<NetworkStop> {
        self.entries = self.entries.saturating_add(visited);
        if self.entries >= self.max_entries {
            Some(NetworkStop::Entries)
        } else if now.saturating_duration_since(self.started) >= self.max_time {
            Some(NetworkStop::Time)
        } else {
            None
        }
    }

    /// Starts a fresh bound after the user chose to continue.
    pub fn resume(&mut self, now: Instant) {
        self.started = now;
        self.entries = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stops_at_the_entry_bound() {
        let now = Instant::now();
        let mut budget = NetworkWalkBudget::with_limits(now, 10, Duration::from_secs(60));
        assert_eq!(budget.visit(9, now), None);
        assert_eq!(budget.visit(1, now), Some(NetworkStop::Entries));
    }

    #[test]
    fn stops_at_the_time_bound() {
        let now = Instant::now();
        let mut budget = NetworkWalkBudget::with_limits(now, 1_000, Duration::from_secs(5));
        assert_eq!(budget.visit(1, now + Duration::from_secs(4)), None);
        assert_eq!(
            budget.visit(1, now + Duration::from_secs(5)),
            Some(NetworkStop::Time)
        );
    }

    #[test]
    fn resume_starts_a_fresh_bound() {
        let now = Instant::now();
        let mut budget = NetworkWalkBudget::with_limits(now, 3, Duration::from_secs(5));
        assert_eq!(budget.visit(3, now), Some(NetworkStop::Entries));
        budget.resume(now);
        assert_eq!(budget.visit(2, now), None);
    }

    #[test]
    fn the_shared_defaults_are_200k_entries_and_60_seconds() {
        assert_eq!(NETWORK_WALK_MAX_ENTRIES, 200_000);
        assert_eq!(NETWORK_WALK_MAX_TIME, Duration::from_secs(60));
    }
}
