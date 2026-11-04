use std::time::{Duration, Instant};

/// Process CPU time from `GetProcessTimes`, in 100 ns ticks.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CpuTime {
    pub kernel_ticks: u64,
    pub user_ticks: u64,
}

impl CpuTime {
    pub(super) fn from_filetimes(kernel: (u32, u32), user: (u32, u32)) -> Self {
        Self {
            kernel_ticks: ticks(kernel.0, kernel.1),
            user_ticks: ticks(user.0, user.1),
        }
    }

    /// Kernel plus user time in milliseconds.
    pub fn total_ms(self) -> f64 {
        to_ms(self.kernel_ticks.saturating_add(self.user_ticks))
    }

    /// CPU milliseconds spent between `earlier` and `self`; zero if the counters went back.
    pub fn ms_since(self, earlier: Self) -> f64 {
        let kernel = self.kernel_ticks.saturating_sub(earlier.kernel_ticks);
        let user = self.user_ticks.saturating_sub(earlier.user_ticks);
        to_ms(kernel.saturating_add(user))
    }
}

/// FILETIME halves (low, high) to one 100 ns count.
pub(super) fn ticks(low: u32, high: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "ticks below 2^53 are exact; larger ones are years of CPU"
)]
fn to_ms(ticks: u64) -> f64 {
    ticks as f64 / 10_000.0
}

/// One thread's context-switch counter at one moment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadSwitches {
    pub thread_id: u32,
    pub switches: u32,
}

/// Context switches of one thread between two samples.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadDelta {
    pub thread_id: u32,
    pub switches: u64,
}

/// Per-thread deltas, largest first. A thread seen only in `after` counts from zero; a
/// counter that went down means a reused thread ID and also counts from zero. Threads that
/// exited before `after` are not counted.
pub fn switch_deltas(before: &[ThreadSwitches], after: &[ThreadSwitches]) -> Vec<ThreadDelta> {
    let mut deltas: Vec<ThreadDelta> = after
        .iter()
        .map(|now| {
            let start = before
                .iter()
                .find(|then| then.thread_id == now.thread_id)
                .map_or(0, |then| then.switches);
            let switches = if now.switches >= start {
                now.switches - start
            } else {
                now.switches
            };
            ThreadDelta {
                thread_id: now.thread_id,
                switches: u64::from(switches),
            }
        })
        .collect();
    deltas.sort_by(|a, b| {
        b.switches
            .cmp(&a.switches)
            .then(a.thread_id.cmp(&b.thread_id))
    });
    deltas
}

/// `count / elapsed` in events per second; `None` for an empty interval.
#[expect(
    clippy::cast_precision_loss,
    reason = "event counts stay far below 2^53"
)]
pub fn per_second(count: u64, elapsed: Duration) -> Option<f64> {
    let seconds = elapsed.as_secs_f64();
    (seconds > 0.0).then(|| count as f64 / seconds)
}

/// Calls `probe` every `interval` until it yields a value or `timeout` has passed.
pub(super) fn poll<T, E>(
    timeout: Duration,
    interval: Duration,
    mut probe: impl FnMut() -> Result<Option<T>, E>,
) -> Result<Option<T>, E> {
    let start = Instant::now();
    loop {
        if let Some(value) = probe()? {
            return Ok(Some(value));
        }
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Ok(None);
        }
        std::thread::sleep(interval.min(timeout - elapsed));
    }
}
