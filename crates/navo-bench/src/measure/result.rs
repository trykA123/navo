use serde::Serialize;

use super::host::MeasureError;
use crate::harness::stats::{Summary, summarize};
use crate::index::micros;
use crate::privacy::SafeText;
use crate::safe_text;

/// A typed number, or the fixed reason it is missing.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Metric<T> {
    Measured { value: T },
    NotMeasured { reason: SafeText },
}

impl<T> Metric<T> {
    pub fn missing(reason: SafeText) -> Self {
        Self::NotMeasured { reason }
    }
}

impl<T> Metric<T> {
    pub(super) fn from_result(result: Result<T, MeasureError>) -> Self {
        match result {
            Ok(value) => Self::Measured { value },
            Err(error) => Self::missing(error.reason()),
        }
    }
}

/// D93 statistics over run times, in microseconds.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Stat {
    pub samples: u32,
    pub median_micros: u64,
    pub p95_micros: u64,
    /// True when fewer than 20 samples make p95 the maximum.
    pub p95_is_max: bool,
    pub iqr_micros: u64,
    pub noisy: bool,
}

impl From<Summary> for Stat {
    fn from(summary: Summary) -> Self {
        Self {
            samples: u32::try_from(summary.count).unwrap_or(u32::MAX),
            median_micros: micros(summary.median),
            p95_micros: micros(summary.p95),
            p95_is_max: summary.p95_is_max,
            iqr_micros: micros(summary.iqr),
            noisy: summary.noisy,
        }
    }
}

/// Statistics over millisecond samples; missing with `empty` when there are none.
pub(crate) fn stat(samples: &[f64], empty: SafeText) -> Metric<Stat> {
    match summarize(samples) {
        Some(summary) => Metric::Measured {
            value: summary.into(),
        },
        None => Metric::missing(empty),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StartKind {
    Cold,
    Warm,
}

#[derive(Clone, Debug, Serialize)]
pub struct StartReport {
    pub kind: StartKind,
    /// Counted runs; the discarded warm-up is not included.
    pub attempted: u32,
    pub failed: u32,
    pub first_window: Metric<Stat>,
    pub first_frame: Metric<Stat>,
    pub first_usable: Metric<Stat>,
}

impl StartReport {
    /// No run could start; all three numbers are missing for one reason.
    pub fn unavailable(kind: StartKind, reason: SafeText) -> Self {
        Self {
            kind,
            attempted: 0,
            failed: 0,
            first_window: Metric::missing(reason),
            first_frame: Metric::missing(reason),
            first_usable: Metric::missing(reason),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Bytes {
    pub private_bytes: u64,
    pub working_set_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CpuWindow {
    pub window_micros: u64,
    pub cpu_micros: u64,
}

/// Context switches per second over the window; the busiest thread stands in for VSync.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Wakeups {
    pub total_per_second: f64,
    pub busiest_thread_per_second: f64,
    pub without_busiest_per_second: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct IdleReport {
    pub memory_at_settle: Metric<Bytes>,
    pub cpu: Metric<CpuWindow>,
    pub wakeups: Metric<Wakeups>,
    pub memory_at_end: Metric<Bytes>,
}

impl IdleReport {
    pub fn unavailable(reason: SafeText) -> Self {
        Self {
            memory_at_settle: Metric::missing(reason),
            cpu: Metric::missing(reason),
            wakeups: Metric::missing(reason),
            memory_at_end: Metric::missing(reason),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "measure", rename_all = "kebab-case")]
pub enum MeasureReport {
    Start(StartReport),
    Idle(IdleReport),
    ExeSize { bytes: Metric<u64> },
}

pub(super) fn no_run() -> SafeText {
    safe_text!("no run succeeded")
}
