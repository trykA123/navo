use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

pub(super) const HUNG_AFTER: Duration = Duration::from_secs(2);
/// Idle work counts as hung only after `HUNG_AFTER` plus this: a slow first load is not a hang.
pub(super) const IDLE_GRACE: Duration = Duration::from_secs(8);

#[derive(Clone, Copy)]
enum Busy {
    Request(Instant),
    Idle(Instant),
}

#[derive(Default)]
struct Marks {
    busy: Option<Busy>,
    waiting: Option<Instant>,
}

/// What the shell thread is doing and since when a posted request waits, shared with the threads that post.
#[derive(Clone, Default)]
pub(super) struct Health(Arc<Mutex<Marks>>);

impl Health {
    fn marks(&self) -> MutexGuard<'_, Marks> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Call before sending a request; only the oldest request not yet taken counts.
    pub(super) fn note_waiting(&self, now: Instant) {
        self.marks().waiting.get_or_insert(now);
    }

    /// Idle work is hung once a request waited `HUNG_AFTER` behind it, or the step ran past its grace.
    pub(super) fn is_hung(&self, now: Instant) -> bool {
        let marks = self.marks();
        let is_older = |since: Instant, limit| now.saturating_duration_since(since) > limit;
        match marks.busy {
            Some(Busy::Request(since)) => is_older(since, HUNG_AFTER),
            Some(Busy::Idle(since)) => {
                is_older(since, HUNG_AFTER + IDLE_GRACE)
                    || marks
                        .waiting
                        .is_some_and(|since| is_older(since, HUNG_AFTER))
            }
            None => false,
        }
    }

    fn mark(&self, is_busy: bool) {
        self.marks().busy = is_busy.then(|| Busy::Request(Instant::now()));
    }

    fn mark_idle(&self) {
        self.marks().busy = Some(Busy::Idle(Instant::now()));
    }

    fn clear_waiting(&self) {
        self.marks().waiting = None;
    }
}

pub(super) trait Station {
    type Request;

    fn can_hang(request: &Self::Request) -> bool;
    fn handle(&mut self, request: Self::Request) -> bool;
    fn settle_fills(&mut self) -> Option<Duration>;
    fn pump(&mut self, timeout: Option<Duration>) -> bool;

    /// One step of low-priority work, run only when no request waits; `false` when there is none.
    fn run_idle(&mut self) -> bool {
        false
    }
}

pub(super) fn serve<S: Station>(station: &mut S, requests: &Receiver<S::Request>, health: &Health) {
    loop {
        while let Ok(request) = requests.try_recv() {
            health.mark(S::can_hang(&request));
            let keep_running = station.handle(request);
            health.mark(false);
            if !keep_running {
                return;
            }
        }
        health.clear_waiting();
        let has_worked = run_idle(station, health);
        let wait = station.settle_fills();
        health.mark(wait.is_some());
        let keep_running = station.pump(if has_worked {
            Some(Duration::ZERO)
        } else {
            wait
        });
        health.mark(false);
        if !keep_running {
            return;
        }
    }
}

fn run_idle<S: Station>(station: &mut S, health: &Health) -> bool {
    health.mark_idle();
    let has_worked = station.run_idle();
    health.mark(false);
    has_worked
}

#[cfg(test)]
mod tests;
