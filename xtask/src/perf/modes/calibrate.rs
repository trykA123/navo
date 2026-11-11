//! SC11's VM baseline: the verdict of one run against it, the calibration line of one run,
//! and the aggregate of ten lines that becomes `perf/vm-baseline.toml`.

use serde::{Deserialize, Serialize};

use super::Run;

#[cfg(test)]
mod tests;

/// SC11's memory limit: a run above `baseline.private_bytes × 1.15` fails the push.
const MEMORY_FACTOR: f64 = 1.15;

/// SC11's time limit, applied only while the baseline carries `time_guard`.
const TIME_FACTOR: f64 = 1.25;

/// SC11's steadiness limits: a spread ratio at or below these is safe to gate on.
const MEMORY_SPREAD_MAX: f64 = 0.05;
const TIME_SPREAD_MAX: f64 = 0.10;

/// SC11: a baseline is the median of ten calibration runs.
const CALIBRATION_RUNS: usize = 10;

/// The prefix of one calibration line, before its JSON object.
const CALIBRATION_PREFIX: &str = "=== calibration ";

/// `perf/vm-baseline.toml`: one calibrated VM's numbers, keys in SC11's order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Baseline {
    /// The VM the ten runs were taken on.
    pub(super) vm_fingerprint: String,
    /// The full sha that was calibrated.
    pub(super) commit: String,
    /// The day the calibration ran, `YYYY-MM-DD`.
    pub(super) measured: String,
    /// Why the baseline changed.
    pub(super) reason: String,
    /// Median of ten runs' largest private reading.
    pub(super) private_bytes: u64,
    /// Median of ten runs' median warm start.
    pub(super) warm_first_usable_ms: f64,
    /// (max − min) / min of the ten private readings.
    pub(super) memory_spread: f64,
    /// (max − min) / min of the ten warm readings.
    pub(super) time_spread: f64,
    /// Memory is steady enough to gate on.
    pub(super) memory_guard: bool,
    /// Time is steady enough to gate on.
    pub(super) time_guard: bool,
}

/// SC11's verdict of one VM run, each carrying the detail line SC11 prints.
#[derive(Debug)]
pub(super) enum Verdict {
    /// Every guard the baseline trusts holds.
    Pass(String),
    /// A guard broke.
    Fail(String),
    /// No baseline this VM can be judged against: SC11's exit 4.
    NotCalibrated(String),
}

/// The four keys of one calibration line; a line with any other key is not one.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Calibration {
    commit: String,
    fingerprint: String,
    private_bytes: u64,
    warm_ms: f64,
}

/// Judge one run against the baseline: SC11's memory factor always, its time factor only
/// under `time_guard`, and "not calibrated" when no baseline can be trusted.
pub(super) fn judge(run: &Run, baseline: Option<&Baseline>, fingerprint: &str) -> Verdict {
    let Some(baseline) = baseline else {
        return Verdict::NotCalibrated("perf/vm-baseline.toml is missing".to_owned());
    };
    if !baseline.memory_guard {
        return Verdict::NotCalibrated(format!(
            "baseline '{}' does not guard memory",
            baseline.measured
        ));
    }
    if baseline.vm_fingerprint != fingerprint {
        return Verdict::NotCalibrated(format!(
            "VM '{fingerprint}' is not the baseline's '{}'",
            baseline.vm_fingerprint
        ));
    }
    let detail = format!(
        "private {} bytes vs baseline {}, warm {:.1} ms vs baseline {:.1}",
        run.private_bytes, baseline.private_bytes, run.warm_ms, baseline.warm_first_usable_ms
    );
    let mut failures = Vec::new();
    if run.private_bytes as f64 > baseline.private_bytes as f64 * MEMORY_FACTOR {
        failures.push(format!(
            "private bytes > {} × {MEMORY_FACTOR}",
            baseline.private_bytes
        ));
    }
    if baseline.time_guard && run.warm_ms > baseline.warm_first_usable_ms * TIME_FACTOR {
        failures.push(format!(
            "warm ms > {:.1} × {TIME_FACTOR}",
            baseline.warm_first_usable_ms
        ));
    }
    if failures.is_empty() {
        Verdict::Pass(detail)
    } else {
        Verdict::Fail(format!("{detail}: {}", failures.join("; ")))
    }
}

/// SC11's calibration line: `=== calibration ` then one compact JSON object of four keys.
pub(super) fn calibration_line(commit: &str, fingerprint: &str, run: &Run) -> String {
    let value = serde_json::json!({
        "commit": commit,
        "fingerprint": fingerprint,
        "private_bytes": run.private_bytes,
        "warm_ms": run.warm_ms,
    });
    format!("{CALIBRATION_PREFIX}{value}")
}

/// Fold SC11's ten calibration lines into the baseline TOML. Blank lines are skipped;
/// every other line must be a calibration line, and the ten must name one commit and one
/// VM, because a baseline describes exactly one of each.
pub(super) fn aggregate(lines: &str, measured: &str, reason: &str) -> Result<String, String> {
    let readings = readings(lines)?;
    if readings.len() != CALIBRATION_RUNS {
        return Err(format!(
            "{} calibration lines, not {CALIBRATION_RUNS}",
            readings.len()
        ));
    }
    let commits: Vec<&str> = readings
        .iter()
        .map(|reading| reading.commit.as_str())
        .collect();
    let fingerprints: Vec<&str> = readings
        .iter()
        .map(|reading| reading.fingerprint.as_str())
        .collect();
    let memory: Vec<f64> = readings
        .iter()
        .map(|reading| reading.private_bytes as f64)
        .collect();
    let mut bytes: Vec<u64> = readings
        .iter()
        .map(|reading| reading.private_bytes)
        .collect();
    let mut warm: Vec<f64> = readings.iter().map(|reading| reading.warm_ms).collect();
    let memory_spread = spread(&memory);
    let time_spread = spread(&warm);
    let baseline = Baseline {
        vm_fingerprint: one(&fingerprints, "fingerprint")?,
        commit: one(&commits, "commit")?,
        measured: measured.to_owned(),
        reason: reason.to_owned(),
        private_bytes: median_u64(&mut bytes),
        warm_first_usable_ms: median_f64(&mut warm),
        memory_spread,
        time_spread,
        memory_guard: memory_spread <= MEMORY_SPREAD_MAX,
        time_guard: time_spread <= TIME_SPREAD_MAX,
    };
    toml::to_string(&baseline).map_err(|error| error.to_string())
}

/// Parse every non-blank line of a lines file as one calibration line.
fn readings(lines: &str) -> Result<Vec<Calibration>, String> {
    lines
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let json = line
                .strip_prefix(CALIBRATION_PREFIX)
                .ok_or_else(|| format!("not a calibration line: {line}"))?;
            serde_json::from_str::<Calibration>(json).map_err(|error| format!("{line}: {error}"))
        })
        .collect()
}

/// SC11: the ten lines must agree on `what`, or they do not describe one calibrated VM.
fn one(values: &[&str], what: &str) -> Result<String, String> {
    let mut distinct = values.to_vec();
    distinct.sort_unstable();
    distinct.dedup();
    match distinct.as_slice() {
        [value] => Ok((*value).to_owned()),
        _ => Err(format!(
            "the ten lines disagree on {what}: {}",
            distinct.join(", ")
        )),
    }
}

/// SC11's median of ten: the mean of the 5th and 6th sorted values, in integer division
/// for bytes. `aggregate` only ever hands these a full ten readings.
fn median_u64(values: &mut [u64]) -> u64 {
    values.sort_unstable();
    let high = values.len() / 2;
    (values[high - 1] + values[high]) / 2
}

/// SC11's median of ten for the warm start times, as `median_u64`.
fn median_f64(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let high = values.len() / 2;
    (values[high - 1] + values[high]) / 2.0
}

/// SC11's spread of a set of readings: the ratio (max − min) / min, never a percent.
fn spread(values: &[f64]) -> f64 {
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (max - min) / min
}
