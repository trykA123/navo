use std::time::Duration;

use super::host::{Exe, Host, MeasureError, Session, Spec};
use super::result::{Bytes, CpuWindow, IdleReport, Metric, Wakeups};
use crate::index::micros;
use crate::probe::{CpuTime, ThreadSwitches, per_second, switch_deltas};
use crate::safe_text;

pub(super) const SETTLE: Duration = Duration::from_secs(30);
pub(super) const WINDOW: Duration = Duration::from_secs(60);
pub(super) const TAIL: Duration = Duration::from_secs(30);
const WINDOW_TIMEOUT: Duration = Duration::from_secs(30);
const CLOSE_DEADLINE: Duration = Duration::from_secs(5);

struct Counters {
    at: Duration,
    cpu: Result<CpuTime, MeasureError>,
    switches: Result<Vec<ThreadSwitches>, MeasureError>,
}

/// One plain launch: memory at 30 s, CPU and wakeups over the next 60 s, memory 30 s later.
pub(super) fn idle(host: &mut dyn Host, run: u32) -> (IdleReport, Option<MeasureError>) {
    let spec = Spec {
        exe: Exe::Original,
        run,
        metrics: false,
    };
    let mut session = match host.launch(spec) {
        Ok(session) => session,
        Err(error) => return (IdleReport::unavailable(error.reason()), Some(error)),
    };
    if let Err(error) = session.wait_for_window(WINDOW_TIMEOUT) {
        return (IdleReport::unavailable(error.reason()), Some(error));
    }
    host.pause(SETTLE);
    let memory_at_settle = bytes(session.as_ref());
    let before = counters(host, session.as_ref());
    host.pause(WINDOW);
    let after = counters(host, session.as_ref());
    host.pause(TAIL);
    let memory_at_end = bytes(session.as_ref());
    // A close that fails still ends in a kill when the session drops.
    let _ = session.close(CLOSE_DEADLINE);
    let report = IdleReport {
        memory_at_settle,
        cpu: cpu_window(&before, &after),
        wakeups: wakeups(&before, &after),
        memory_at_end,
    };
    (report, None)
}

fn bytes(session: &dyn Session) -> Metric<Bytes> {
    Metric::from_result(session.memory().map(|memory| Bytes {
        private_bytes: memory.private_bytes,
        working_set_bytes: memory.working_set_bytes,
    }))
}

fn counters(host: &dyn Host, session: &dyn Session) -> Counters {
    Counters {
        at: host.elapsed(),
        cpu: session.cpu_time(),
        switches: session.thread_switches(),
    }
}

fn cpu_window(before: &Counters, after: &Counters) -> Metric<CpuWindow> {
    let result = match (&before.cpu, &after.cpu) {
        (Ok(first), Ok(last)) => Ok(CpuWindow {
            window_micros: u64::try_from(window(before, after).as_micros()).unwrap_or(u64::MAX),
            cpu_micros: micros(last.ms_since(*first)),
        }),
        (Err(error), _) | (_, Err(error)) => Err(*error),
    };
    Metric::from_result(result)
}

fn wakeups(before: &Counters, after: &Counters) -> Metric<Wakeups> {
    let (Ok(first), Ok(last)) = (&before.switches, &after.switches) else {
        return Metric::missing(safe_text!("thread counters unreadable"));
    };
    let deltas = switch_deltas(first, last);
    let total: u64 = deltas.iter().map(|delta| delta.switches).sum();
    let busiest = deltas.first().map_or(0, |delta| delta.switches);
    let elapsed = window(before, after);
    let rates = (
        per_second(total, elapsed),
        per_second(busiest, elapsed),
        per_second(total.saturating_sub(busiest), elapsed),
    );
    match rates {
        (Some(total_per_second), Some(busiest_thread_per_second), Some(without)) => {
            Metric::Measured {
                value: Wakeups {
                    total_per_second,
                    busiest_thread_per_second,
                    without_busiest_per_second: without,
                },
            }
        }
        _ => Metric::missing(safe_text!("empty measuring window")),
    }
}

fn window(before: &Counters, after: &Counters) -> Duration {
    after.at.saturating_sub(before.at)
}
