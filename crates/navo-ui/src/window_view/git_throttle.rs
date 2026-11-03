use std::time::{Duration, Instant};

/// Panes re-render at most this often while Git results stream in.
const INTERVAL: Duration = Duration::from_millis(100);

/// What to do with one badge publish.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Admit {
    /// Refresh the panes now.
    Now,
    /// Refresh once after this delay; it also covers later publishes until then.
    Later(Duration),
    /// A delayed refresh is already waiting.
    Covered,
}

#[derive(Default)]
pub(super) struct Throttle {
    last: Option<Instant>,
    waiting: bool,
}

impl Throttle {
    pub(super) fn admit(&mut self, now: Instant) -> Admit {
        if self.waiting {
            return Admit::Covered;
        }
        let since = self.last.map(|last| now.saturating_duration_since(last));
        match since {
            Some(since) if since < INTERVAL => {
                self.waiting = true;
                Admit::Later(INTERVAL - since)
            }
            _ => {
                self.last = Some(now);
                Admit::Now
            }
        }
    }

    /// The delayed refresh ran.
    pub(super) fn fired(&mut self, now: Instant) {
        self.waiting = false;
        self.last = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn the_first_publish_is_immediate_and_a_burst_costs_one_more_refresh() {
        let start = Instant::now();
        let mut throttle = Throttle::default();
        assert_eq!(throttle.admit(at(start, 0)), Admit::Now);
        assert_eq!(
            throttle.admit(at(start, 30)),
            Admit::Later(Duration::from_millis(70))
        );
        for millis in [40, 60, 99] {
            assert_eq!(throttle.admit(at(start, millis)), Admit::Covered);
        }
    }

    #[test]
    fn after_the_delayed_refresh_the_next_publish_waits_a_full_interval() {
        let start = Instant::now();
        let mut throttle = Throttle::default();
        throttle.admit(at(start, 0));
        throttle.admit(at(start, 30));
        throttle.fired(at(start, 100));
        assert_eq!(
            throttle.admit(at(start, 150)),
            Admit::Later(Duration::from_millis(50))
        );
    }

    #[test]
    fn a_quiet_spell_makes_the_next_publish_immediate_again() {
        let start = Instant::now();
        let mut throttle = Throttle::default();
        throttle.admit(at(start, 0));
        assert_eq!(throttle.admit(at(start, 100)), Admit::Now);
        assert_eq!(throttle.admit(at(start, 1_000)), Admit::Now);
    }
}
