use std::io;

use crate::guard::GuardError;
use crate::measure::MetricsError;
use crate::privacy::SafeText;
use crate::probe::ProbeError;
use crate::safe_text;

/// Why one timing step failed. Holds roles and codes only, never a path or a name.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TimingError {
    #[error("timings:{0}")]
    Probe(#[from] ProbeError),
    #[error("timings:{0}")]
    Guard(#[from] GuardError),
    #[error("timings:{0}")]
    Metrics(#[from] MetricsError),
    #[error("timings:io-{0:?}")]
    Io(io::ErrorKind),
    #[error("timings:bench-exit")]
    BenchExit,
    #[error("timings:bench-timeout")]
    BenchTimeout,
    #[error("timings:bench-json")]
    BenchJson,
    #[error("timings:metrics-missing")]
    MetricsMissing,
    #[error("timings:trace-missing")]
    TraceMissing,
    #[error("timings:no-folder")]
    NoFolder,
    #[error("timings:not-built")]
    NotBuilt,
    #[error("timings:not-prepared")]
    NotPrepared,
}

impl TimingError {
    /// The code a failed phase records.
    pub fn code(self) -> u32 {
        match self {
            Self::Probe(_) => 1,
            Self::Guard(_) => 2,
            Self::Metrics(_) => 3,
            Self::Io(_) => 4,
            Self::BenchExit => 5,
            Self::BenchTimeout => 6,
            Self::BenchJson => 7,
            Self::MetricsMissing => 8,
            Self::TraceMissing => 9,
            Self::NoFolder => 10,
            Self::NotBuilt => 11,
            Self::NotPrepared => 12,
        }
    }

    /// Fixed words for `not measured(reason)`.
    pub fn reason(self) -> SafeText {
        match self {
            Self::Probe(ProbeError::Exited) => safe_text!("process exited early"),
            Self::Probe(ProbeError::NoWindow) => safe_text!("no window appeared"),
            Self::Probe(ProbeError::Unsupported) => safe_text!("probes need Windows"),
            Self::Probe(_) => safe_text!("probe failed"),
            Self::Guard(_) | Self::Io(_) => safe_text!("scratch folder not writable"),
            Self::Metrics(_) => safe_text!("metrics file unreadable"),
            Self::MetricsMissing => safe_text!("build wrote no metrics file"),
            Self::TraceMissing => safe_text!("build wrote no trace records"),
            Self::BenchExit => safe_text!("folder benchmark failed"),
            Self::BenchTimeout => safe_text!("folder benchmark timed out"),
            Self::BenchJson => safe_text!("folder benchmark output unreadable"),
            Self::NoFolder => safe_text!("no folder chosen"),
            Self::NotBuilt => safe_text!("100k folder not built"),
            Self::NotPrepared => safe_text!("working folders not built"),
        }
    }
}
