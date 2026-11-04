use std::time::Duration;

use super::host::{Exe, Host, MeasureError, Spec};
use super::metrics::{MetricsError, StartMetrics, parse_metrics};
use super::result::{StartKind, StartReport, no_run, stat};
use crate::safe_text;

pub(super) const COLD_RUNS: u32 = 3;
pub(super) const WARM_DISCARDED: u32 = 1;
pub(super) const WARM_RUNS: u32 = 20;
const WINDOW_TIMEOUT: Duration = Duration::from_secs(30);
/// Navo writes `--metrics` about 5 s after the first usable frame.
const METRICS_TIMEOUT: Duration = Duration::from_secs(20);
const METRICS_POLL: Duration = Duration::from_millis(100);
const CLOSE_DEADLINE: Duration = Duration::from_secs(5);

/// Hands out run numbers; each start gets a fresh profile and metrics file.
#[derive(Default)]
pub(super) struct Ids(u32);

impl Ids {
    pub(super) fn next(&mut self) -> u32 {
        self.0 += 1;
        self.0
    }
}

pub(super) struct Outcome {
    pub(super) report: StartReport,
    /// The last run error; set only when no run succeeded.
    pub(super) failure: Option<MeasureError>,
}

struct Sample {
    window_ms: f64,
    metrics: Result<StartMetrics, MeasureError>,
}

/// Three launches, each from its own fresh copy of the exe.
pub(super) fn cold(host: &mut dyn Host, ids: &mut Ids) -> Outcome {
    let mut tally = Tally::default();
    for _ in 0..COLD_RUNS {
        let copy = ids.next();
        let spec = Spec {
            exe: Exe::Copy(copy),
            run: copy,
            metrics: true,
        };
        tally.add(host.copy_exe(copy).and_then(|()| run_once(host, spec)));
    }
    tally.finish(StartKind::Cold)
}

/// One discarded launch, then twenty from the same exe.
pub(super) fn warm(host: &mut dyn Host, ids: &mut Ids) -> Outcome {
    let mut tally = Tally::default();
    for round in 0..WARM_DISCARDED + WARM_RUNS {
        let spec = Spec {
            exe: Exe::Original,
            run: ids.next(),
            metrics: true,
        };
        let result = run_once(host, spec);
        if round >= WARM_DISCARDED {
            tally.add(result);
        }
    }
    tally.finish(StartKind::Warm)
}

fn run_once(host: &mut dyn Host, spec: Spec) -> Result<Sample, MeasureError> {
    let mut session = host.launch(spec)?;
    let window = session.wait_for_window(WINDOW_TIMEOUT)?;
    let metrics = if spec.metrics {
        await_metrics(host, spec.run)
    } else {
        Err(MeasureError::MetricsMissing)
    };
    // A close that fails still ends in a kill when the session drops.
    let _ = session.close(CLOSE_DEADLINE);
    Ok(Sample {
        window_ms: window.as_secs_f64() * 1000.0,
        metrics,
    })
}

fn await_metrics(host: &mut dyn Host, run: u32) -> Result<StartMetrics, MeasureError> {
    let began = host.elapsed();
    let mut last = MeasureError::MetricsMissing;
    while host.elapsed().saturating_sub(began) < METRICS_TIMEOUT {
        if let Some(text) = host.read_metrics(run) {
            match parse_metrics(&text) {
                Ok(metrics) => return Ok(metrics),
                Err(MetricsError::Empty) => return Err(MetricsError::Empty.into()),
                // A half-written file parses on the next poll.
                Err(error) => last = error.into(),
            }
        }
        host.pause(METRICS_POLL);
    }
    Err(last)
}

#[derive(Default)]
struct Tally {
    attempted: u32,
    failed: u32,
    windows: Vec<f64>,
    frames: Vec<f64>,
    usables: Vec<f64>,
    metrics_error: Option<MeasureError>,
    last_error: Option<MeasureError>,
}

impl Tally {
    fn add(&mut self, result: Result<Sample, MeasureError>) {
        self.attempted += 1;
        match result {
            Ok(sample) => self.keep(sample),
            Err(error) => {
                self.failed += 1;
                self.last_error = Some(error);
            }
        }
    }

    fn keep(&mut self, sample: Sample) {
        self.windows.push(sample.window_ms);
        match sample.metrics {
            Ok(metrics) => {
                self.frames.extend(metrics.first_frame_ms);
                self.usables.extend(metrics.first_usable_ms);
            }
            Err(error) => self.metrics_error = Some(error),
        }
    }

    fn finish(self, kind: StartKind) -> Outcome {
        if self.windows.is_empty() {
            let reason = self.last_error.map_or_else(no_run, MeasureError::reason);
            let mut report = StartReport::unavailable(kind, reason);
            report.attempted = self.attempted;
            report.failed = self.failed;
            return Outcome {
                report,
                failure: self.last_error,
            };
        }
        let why = self.metrics_error.map_or(
            safe_text!("build wrote no start numbers"),
            MeasureError::reason,
        );
        let report = StartReport {
            kind,
            attempted: self.attempted,
            failed: self.failed,
            first_window: stat(&self.windows, no_run()),
            first_frame: stat(&self.frames, why),
            first_usable: stat(&self.usables, why),
        };
        Outcome {
            report,
            failure: None,
        }
    }
}
