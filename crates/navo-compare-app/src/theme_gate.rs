//! Rate gate for theme changes (NC153): at most one applied palette or mode change per
//! `MIN_GAP`, so repeated clicks can never flash the window.
use std::time::{Duration, Instant};

/// The shortest time between two applied theme changes: two per second, under the 3 Hz limit.
pub const MIN_GAP: Duration = Duration::from_millis(500);

/// What a theme request should do now: apply, schedule one trailing apply, or wait for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateStep {
    /// Apply now; the gate stamped this time.
    Now,
    /// Schedule one apply after this delay, then call `fired`.
    After(Duration),
    /// A scheduled apply is already waiting; it will read the latest choice.
    Waiting,
}

/// Remembers the last applied change and whether one trailing apply is scheduled.
#[derive(Debug, Default)]
pub struct ThemeGate {
    last: Option<Instant>,
    pending: bool,
}

impl ThemeGate {
    /// pending → Waiting. No `last`, or `now - last >= MIN_GAP` → stamp `now`, Now.
    /// Otherwise → pending = true, After(MIN_GAP - (now - last)). Uses `saturating_duration_since`.
    pub fn request(&mut self, now: Instant) -> GateStep {
        if self.pending {
            return GateStep::Waiting;
        }
        if let Some(last) = self.last {
            let elapsed = now.saturating_duration_since(last);
            if elapsed < MIN_GAP {
                self.pending = true;
                return GateStep::After(MIN_GAP - elapsed);
            }
        }
        self.last = Some(now);
        GateStep::Now
    }

    /// The trailing apply ran at `now`: pending = false, last = now.
    pub fn fired(&mut self, now: Instant) {
        self.pending = false;
        self.last = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A first request has no prior stamp, so it applies at once.
    #[test]
    fn the_first_request_applies_now() {
        let mut gate = ThemeGate::default();
        assert_eq!(gate.request(Instant::now()), GateStep::Now);
    }

    /// A request 200 ms after an apply waits out the remaining 300 ms of the gap.
    #[test]
    fn a_request_inside_the_gap_waits_for_the_rest() {
        let t0 = Instant::now();
        let mut gate = ThemeGate::default();
        assert_eq!(gate.request(t0), GateStep::Now);
        assert_eq!(
            gate.request(t0 + Duration::from_millis(200)),
            GateStep::After(Duration::from_millis(300))
        );
    }

    /// A second request while a trailing apply is pending is folded into it.
    #[test]
    fn a_second_request_while_pending_is_waiting() {
        let t0 = Instant::now();
        let mut gate = ThemeGate::default();
        assert_eq!(gate.request(t0), GateStep::Now);
        assert_eq!(
            gate.request(t0 + Duration::from_millis(200)),
            GateStep::After(Duration::from_millis(300))
        );
        assert_eq!(
            gate.request(t0 + Duration::from_millis(300)),
            GateStep::Waiting
        );
    }

    /// The trailing apply restarts the gap from the moment it actually ran.
    #[test]
    fn after_firing_the_gap_restarts() {
        let t0 = Instant::now();
        let mut gate = ThemeGate::default();
        assert_eq!(gate.request(t0), GateStep::Now);
        gate.fired(t0 + Duration::from_millis(500));
        assert_eq!(
            gate.request(t0 + Duration::from_millis(600)),
            GateStep::After(Duration::from_millis(400))
        );
        gate.fired(t0 + Duration::from_millis(1000));
        assert_eq!(
            gate.request(t0 + Duration::from_millis(1500)),
            GateStep::Now
        );
    }

    /// A request exactly one gap after the last apply is allowed through.
    #[test]
    fn a_request_at_exactly_the_gap_applies_now() {
        let t0 = Instant::now();
        let mut gate = ThemeGate::default();
        assert_eq!(gate.request(t0), GateStep::Now);
        assert_eq!(gate.request(t0 + MIN_GAP), GateStep::Now);
    }
}
