use super::results::Results;
use super::targets::{INDEX_TARGET_ENTRIES, Observed, TargetId, TargetSpec};
use super::timings::{TimingFolder, TimingRow, TimingStep};
use crate::index::IndexReport;
use crate::measure::{Metric, StartReport, Stat};
use crate::privacy::SafeText;
use crate::safe_text;

fn not_run() -> SafeText {
    safe_text!("this measure did not run")
}

fn metric<T>(metric: &Metric<T>, pick: impl FnOnce(&T) -> u64) -> Observed {
    match metric {
        Metric::Measured { value } => Ok(pick(value)),
        Metric::NotMeasured { reason } => Err(*reason),
    }
}

fn start(report: Option<&StartReport>, pick: fn(&StartReport) -> &Metric<Stat>) -> Observed {
    let report = report.ok_or_else(not_run)?;
    metric(pick(report), |stat| stat.median_micros)
}

fn milli_per_second(rate: f64) -> u64 {
    if rate.is_finite() && rate > 0.0 {
        (rate * 1000.0).round() as u64
    } else {
        0
    }
}

fn idle_rate(results: &Results, pick: fn(&crate::measure::Wakeups) -> f64) -> Observed {
    let idle = results.idle.as_ref().ok_or_else(not_run)?;
    metric(&idle.wakeups, |wakeups| milli_per_second(pick(wakeups)))
}

fn timing(results: &Results, keep: impl Fn(&TimingRow) -> Option<u64>) -> Observed {
    results
        .timings
        .rows()
        .iter()
        .filter_map(keep)
        .max()
        .ok_or_else(|| results.timings.missing_reason())
}

fn ctrl_k(index: &IndexReport) -> Observed {
    index
        .queries
        .iter()
        .map(|query| query.median_micros)
        .max()
        .ok_or(safe_text!("the index answered no query"))
}

fn index_memory(index: &IndexReport, spec: &TargetSpec) -> Observed {
    let bytes = index
        .memory_bytes()
        .ok_or(safe_text!("the index service reported no memory"))?;
    if bytes >= spec.limit || index.entries >= INDEX_TARGET_ENTRIES {
        Ok(bytes)
    } else {
        Err(safe_text!("the index holds fewer than 1 M files"))
    }
}

/// The measured number for a target, or why there is none.
pub fn observe(spec: &TargetSpec, results: &Results) -> Observed {
    let index = || results.index.as_ref().ok_or_else(not_run);
    match spec.id {
        TargetId::ColdUsable | TargetId::ColdUsableSpeedPhase => {
            start(results.cold.as_ref(), |r| &r.first_usable)
        }
        TargetId::WarmFirstFrame => start(results.warm.as_ref(), |r| &r.first_frame),
        TargetId::WarmFirstUsable => start(results.warm.as_ref(), |r| &r.first_usable),
        TargetId::IdlePrivateBytes => {
            let idle = results.idle.as_ref().ok_or_else(not_run)?;
            metric(&idle.memory_at_settle, |bytes| bytes.private_bytes)
        }
        TargetId::IdleWakeupsFocused => idle_rate(results, |w| w.without_busiest_per_second),
        TargetId::IdleWakeupsMinimised => Err(safe_text!("minimised wakeups are not recorded")),
        TargetId::KeyToFrameP95 => timing(results, |row| {
            (row.step == TimingStep::KeyToFrame).then_some(row.p95_micros)
        }),
        TargetId::Open100k => timing(results, |row| {
            (row.folder == TimingFolder::Generated100k && row.step == TimingStep::List)
                .then_some(row.median_micros)
        }),
        TargetId::CtrlK => ctrl_k(index()?),
        TargetId::IndexMemory => index_memory(index()?, spec),
        TargetId::ExeSize => match results.exe_size.as_ref() {
            Some(size) => metric(size, |bytes| *bytes),
            None => Err(not_run()),
        },
        TargetId::TraySummon => Err(safe_text!("not measured in v1")),
    }
}
