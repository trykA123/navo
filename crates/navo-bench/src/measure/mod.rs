//! Start and idle measures of a tester Navo: cold and warm start, idle memory, CPU and
//! wakeups, and exe size, on fixtures A and B built through the guard.
//!
//! Every measure reports a typed number or `not measured(reason)` into `privacy::Report`.
//! Probes and the clock sit behind `Host`, so the logic runs against fakes.

mod host;
mod idle;
mod layout;
mod metrics;
mod names;
mod real;
mod result;
mod run;
mod start;

pub use host::{Exe, Host, MeasureError, Session, Spec};
pub use metrics::{MetricsError, StartMetrics, parse_metrics};
pub(crate) use names::Names;
pub use real::RealHost;
pub(crate) use result::stat;
pub use result::{
    Bytes, CpuWindow, IdleReport, MeasureReport, Metric, StartKind, StartReport, Stat, Wakeups,
};
pub use run::measure_start_and_idle;

#[cfg(test)]
pub(crate) mod fake;
#[cfg(test)]
mod metrics_tests;
#[cfg(test)]
mod real_tests;
#[cfg(test)]
mod tests;
