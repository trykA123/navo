use std::time::Duration;

use super::error::TimingError;
use super::host::{Capture, Host, Launch, Subject, Window};
use super::keys::keys;
use super::result::{KeysReport, UsableReport};
use super::run::Ids;
use super::trace::parse_trace;
use crate::measure::{MetricsError, parse_metrics, stat};
use crate::safe_text;

pub(super) const RUNS: u32 = 5;
const WINDOW_TIMEOUT: Duration = Duration::from_secs(30);
/// Navo writes `--metrics` about 5 s after the first usable frame; a 100k folder may be slower.
const METRICS_TIMEOUT: Duration = Duration::from_secs(60);
const METRICS_POLL: Duration = Duration::from_millis(100);
const SETTLE: Duration = Duration::from_millis(500);
/// Long enough for each key to get its own frame.
const BETWEEN_KEYS: Duration = Duration::from_millis(150);
const TAIL: Duration = Duration::from_millis(500);
const CLOSE_DEADLINE: Duration = Duration::from_secs(5);

fn await_first_usable(host: &mut dyn Host, run: u32) -> Result<f64, TimingError> {
    let began = host.elapsed();
    let mut last = TimingError::MetricsMissing;
    while host.elapsed().saturating_sub(began) < METRICS_TIMEOUT {
        if let Some(text) = host.read(Capture::Metrics, run) {
            match parse_metrics(&text) {
                Ok(metrics) => return metrics.first_usable_ms.ok_or(MetricsError::Empty.into()),
                Err(MetricsError::Empty) => return Err(MetricsError::Empty.into()),
                // A half-written file parses on the next poll.
                Err(error) => last = error.into(),
            }
        }
        host.pause(METRICS_POLL);
    }
    Err(last)
}

fn usable_once(host: &mut dyn Host, subject: Subject, run: u32) -> Result<f64, TimingError> {
    let launch = Launch {
        subject,
        run,
        metrics: true,
        trace: false,
    };
    let mut window = host.launch(launch)?;
    window.wait_for_window(WINDOW_TIMEOUT)?;
    let usable = await_first_usable(host, run);
    // A close that fails still ends in a kill when the window drops.
    let _ = window.close(CLOSE_DEADLINE);
    usable
}

/// Five launches with the folder in the left pane; first usable from `--metrics`.
pub(super) fn first_usable(
    host: &mut dyn Host,
    ids: &mut Ids,
    subject: Subject,
) -> (UsableReport, Option<TimingError>) {
    let (mut samples, mut failed, mut last) = (Vec::new(), 0, None);
    for _ in 0..RUNS {
        match usable_once(host, subject, ids.next()) {
            Ok(ms) => samples.push(ms),
            Err(error) => {
                failed += 1;
                last = Some(error);
            }
        }
    }
    let why = last.map_or(
        safe_text!("build wrote no start numbers"),
        TimingError::reason,
    );
    let report = UsableReport {
        subject,
        attempted: RUNS,
        failed,
        first_usable: stat(&samples, why),
    };
    (report, (samples.is_empty()).then_some(last).flatten())
}

fn type_keys(host: &mut dyn Host, window: &mut dyn Window) -> Result<u32, TimingError> {
    let keys = keys();
    host.pause(SETTLE);
    for key in &keys {
        window.post_keys(std::slice::from_ref(key))?;
        host.pause(BETWEEN_KEYS);
    }
    host.pause(TAIL);
    Ok(u32::try_from(keys.len()).unwrap_or(u32::MAX))
}

/// One launch: wait until usable, type the letters, close, read the trace back.
fn keys_once(
    host: &mut dyn Host,
    subject: Subject,
    run: u32,
) -> Result<(u32, Vec<f64>), TimingError> {
    let launch = Launch {
        subject,
        run,
        metrics: true,
        trace: true,
    };
    let mut window = host.launch(launch)?;
    window.wait_for_window(WINDOW_TIMEOUT)?;
    await_first_usable(host, run)?;
    // The splash window seen first is gone by now; target the window that stayed.
    window.wait_for_window(WINDOW_TIMEOUT)?;
    let posted = type_keys(host, window.as_mut());
    let _ = window.close(CLOSE_DEADLINE);
    let text = host
        .read(Capture::Trace, run)
        .ok_or(TimingError::TraceMissing)?;
    Ok((posted?, parse_trace(&text).key_to_frame_ms))
}

/// Five launches with `--trace`; plain letters posted to the window, key-to-frame from the trace.
pub(super) fn key_to_frame(
    host: &mut dyn Host,
    ids: &mut Ids,
    subject: Subject,
) -> (KeysReport, Option<TimingError>) {
    let (mut samples, mut posted, mut failed, mut last) = (Vec::new(), 0, 0, None);
    for _ in 0..RUNS {
        match keys_once(host, subject, ids.next()) {
            Ok((count, mut times)) => {
                posted += count;
                samples.append(&mut times);
            }
            Err(error) => {
                failed += 1;
                last = Some(error);
            }
        }
    }
    let why = last.map_or(
        safe_text!("build traced no key events"),
        TimingError::reason,
    );
    let report = KeysReport {
        subject,
        attempted: RUNS,
        failed,
        keys_posted: posted,
        records: u32::try_from(samples.len()).unwrap_or(u32::MAX),
        key_to_frame: stat(&samples, why),
    };
    (report, (failed == RUNS).then_some(last).flatten())
}
