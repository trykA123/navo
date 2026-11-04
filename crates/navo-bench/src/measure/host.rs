use std::io;
use std::time::Duration;

use super::metrics::MetricsError;
use crate::guard::GuardError;
use crate::privacy::SafeText;
use crate::probe::{CloseOutcome, CpuTime, Memory, ProbeError, ThreadSwitches};
use crate::safe_text;

/// Why one measure step failed. Holds codes only, never a path or a name.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum MeasureError {
    #[error("measure:{0}")]
    Probe(#[from] ProbeError),
    #[error("measure:{0}")]
    Guard(#[from] GuardError),
    #[error("measure:{0}")]
    Metrics(#[from] MetricsError),
    #[error("measure:io-{0:?}")]
    Io(io::ErrorKind),
    #[error("measure:metrics-file-missing")]
    MetricsMissing,
}

impl MeasureError {
    /// The code a failed phase records.
    pub fn code(self) -> u32 {
        match self {
            Self::Probe(_) => 1,
            Self::Guard(_) => 2,
            Self::Metrics(_) => 3,
            Self::Io(_) => 4,
            Self::MetricsMissing => 5,
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
        }
    }
}

/// Which navo.exe to start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Exe {
    /// The tester build beside navo-bench.
    Original,
    /// The numbered fresh copy inside the run root.
    Copy(u32),
}

/// One Navo start: fresh profile `run`, A on the left, B on the right.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Spec {
    pub exe: Exe,
    pub run: u32,
    pub metrics: bool,
}

/// A running Navo. Dropping it ends the process tree.
pub trait Session {
    fn wait_for_window(&mut self, timeout: Duration) -> Result<Duration, MeasureError>;
    fn memory(&self) -> Result<Memory, MeasureError>;
    fn cpu_time(&self) -> Result<CpuTime, MeasureError>;
    fn thread_switches(&self) -> Result<Vec<ThreadSwitches>, MeasureError>;
    fn close(self: Box<Self>, deadline: Duration) -> Result<CloseOutcome, MeasureError>;
}

/// Everything a measure needs from the machine; tests inject a fake.
pub trait Host {
    /// Builds the working folders and fixtures A and B.
    fn prepare(&mut self) -> Result<(), MeasureError>;
    /// Copies navo.exe (and pdfium.dll when beside it) to a fresh numbered place.
    fn copy_exe(&mut self, copy: u32) -> Result<(), MeasureError>;
    fn launch(&mut self, spec: Spec) -> Result<Box<dyn Session>, MeasureError>;
    /// The text of the run's `--metrics` file, once it exists.
    fn read_metrics(&mut self, run: u32) -> Option<String>;
    fn pause(&mut self, time: Duration);
    /// Time since the host was made.
    fn elapsed(&self) -> Duration;
    fn exe_size(&self) -> Result<u64, MeasureError>;
}
