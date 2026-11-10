//! Compare speed manifests and instruction counts under guard and noise rules.

use std::collections::BTreeSet;

use super::{
    config::{Config, Retired},
    summary::Measured,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Guard,
    Noise,
}

#[derive(Debug)]
pub struct Report {
    pub lines: Vec<String>,
    pub failed: bool,
}

impl Report {
    /// Record a failed rule without discarding the remaining findings.
    fn fail(&mut self, finding: String) {
        self.failed = true;
        self.lines.push(finding);
    }
}

/// Compare counts and manifests using permille limits; return a table and all findings.
#[expect(
    clippy::too_many_arguments,
    reason = "A3 specifies the pure verdict inputs explicitly"
)]
pub fn verdict(
    measured: &[Measured],
    head: &Config,
    base: Option<&Config>,
    retired: &[Retired],
    base_sha: &str,
    targets: &[String],
    mode: Mode,
    limit: u64,
    noise_limit: u64,
    resolve: impl Fn(&str) -> Option<String>,
) -> Report {
    let mut rows: Vec<_> = measured.iter().collect();
    rows.sort_by(|left, right| left.id.cmp(&right.id));
    let mut report = table(&rows);
    if mode == Mode::Guard && base.is_none() {
        report
            .lines
            .insert(0, "no base manifest; no established IDs".into());
    }
    let guard = Guard {
        head,
        base,
        retired,
        base_sha,
        limit,
    };
    if mode == Mode::Guard {
        check_manifest(measured, head, targets, &mut report);
        check_established(&guard, &resolve, &mut report);
        for entry in &head.accepted {
            if !measured.iter().any(|item| item.id == entry.bench) {
                report.fail(format!("stale accepted {}: no measured ID", entry.bench));
            } else if resolve(&entry.base).as_deref() != Some(base_sha) {
                report.lines.push(format!(
                    "warning: stale accepted {} at {}: {}",
                    entry.bench, entry.base, entry.reason
                ));
            }
        }
    }
    for item in rows {
        match mode {
            Mode::Guard => compare_guard(item, &guard, &resolve, &mut report),
            Mode::Noise => compare_noise(item, head, noise_limit, &mut report),
        }
    }
    report
}

struct Guard<'a> {
    head: &'a Config,
    base: Option<&'a Config>,
    retired: &'a [Retired],
    base_sha: &'a str,
    limit: u64,
}

/// Check manifest coverage and uniqueness against measured IDs and discovered targets.
fn check_manifest(items: &[Measured], head: &Config, targets: &[String], report: &mut Report) {
    if head.benches.is_empty() {
        report.fail("head manifest is empty".into());
    }
    let mut manifest_ids = BTreeSet::new();
    let mut measured_ids = BTreeSet::new();
    for bench in &head.benches {
        if !manifest_ids.insert(bench.id.as_str()) {
            report.fail(format!("duplicate manifest ID: {}", bench.id));
        }
    }
    for item in items {
        if !measured_ids.insert(item.id.as_str()) {
            report.fail(format!("duplicate measured ID: {}", item.id));
        }
        if !manifest_ids.contains(item.id.as_str()) {
            report.fail(format!("unlisted ID: {}", item.id));
        }
    }
    for bench in &head.benches {
        if !measured_ids.contains(bench.id.as_str()) {
            report.fail(format!("{}: no head summary", bench.id));
        }
    }
    check_targets(head, targets, report);
}

/// Require coverage in both directions between manifest IDs and speed-targets lines.
fn check_targets(head: &Config, targets: &[String], report: &mut Report) {
    let mut target_ids = BTreeSet::new();
    for target in targets {
        let fields: Vec<_> = target.split_whitespace().collect();
        if fields.len() != 3 {
            report.fail(format!("invalid target line: {target}"));
            continue;
        }
        target_ids.insert(format!("{}/{}", fields[0], fields[1]));
    }
    let mut manifest_targets = BTreeSet::new();
    for bench in &head.benches {
        let target = bench.id.split("::").next().unwrap_or_default();
        if !target_ids.contains(target) {
            report.fail(format!("{}: names no target", bench.id));
        }
        manifest_targets.insert(target);
    }
    for target in target_ids {
        if !manifest_targets.contains(target.as_str()) {
            report.fail(format!("target {target} has no manifest ID"));
        }
    }
}

/// Enforce established IDs unless a reviewed retirement resolves to this run's base.
fn check_established(
    guard: &Guard<'_>,
    resolve: &impl Fn(&str) -> Option<String>,
    report: &mut Report,
) {
    let Some(base) = guard.base else {
        return;
    };
    for entry in base.benches.iter().filter(|entry| entry.guarded) {
        if guard
            .head
            .benches
            .iter()
            .any(|head| head.id == entry.id && head.guarded)
        {
            continue;
        }
        let retirement = guard.retired.iter().find(|retired| {
            retired.id == entry.id
                && !retired.reason.trim().is_empty()
                && resolve(&retired.base).as_deref() == Some(guard.base_sha)
        });
        match retirement {
            Some(retired) => report
                .lines
                .push(format!("retired {}: {}", entry.id, retired.reason)),
            None => report.fail(format!(
                "established ID removed or downgraded: {}",
                entry.id
            )),
        }
    }
}

/// Apply guarded, bootstrap, and accepted-slowdown rules to one measured ID.
fn compare_guard(
    item: &Measured,
    guard: &Guard<'_>,
    resolve: &impl Fn(&str) -> Option<String>,
    report: &mut Report,
) {
    let Some(entry) = guard.head.benches.iter().find(|entry| entry.id == item.id) else {
        return;
    };
    let Some(old) = item.old_ir else {
        if entry.guarded {
            report.fail(format!("{}: no base Ir", item.id));
        } else {
            report.lines.push(format!("{}: bootstrap", item.id));
        }
        return;
    };
    if !entry.guarded {
        report
            .lines
            .push(format!("{}: set guarded = true", item.id));
    }
    if item.new_ir <= old || !exceeds(item.new_ir - old, old, guard.limit) {
        return;
    }
    check_regression(item, guard, resolve, report);
}

/// Reject an above-limit slowdown unless its accepted entry applies to this base and count.
fn check_regression(
    item: &Measured,
    guard: &Guard<'_>,
    resolve: &impl Fn(&str) -> Option<String>,
    report: &mut Report,
) {
    let diff = item
        .old_ir
        .map(|old| difference(item.new_ir, old))
        .unwrap_or_default();
    let accepted = guard.head.accepted.iter().find(|entry| {
        entry.bench == item.id
            && !entry.reason.trim().is_empty()
            && entry.max_percent.is_finite()
            && resolve(&entry.base).as_deref() == Some(guard.base_sha)
            && super::percent(&entry.max_percent.to_string()).is_ok_and(|limit| {
                item.old_ir
                    .is_some_and(|old| !exceeds(item.new_ir.saturating_sub(old), old, limit))
            })
    });
    match accepted {
        Some(entry) => report.lines.push(format!(
            "{}: accepted {diff:.3}%: {}",
            item.id, entry.reason
        )),
        None => report.fail(format!(
            "{}: regressed {diff:.3}% > {:.3}%",
            item.id,
            guard.limit as f64 / 10.0
        )),
    }
}

/// Compare both noise runs, allowing unlisted IDs while requiring both readings.
fn compare_noise(item: &Measured, head: &Config, limit: u64, report: &mut Report) {
    if !head.benches.iter().any(|entry| entry.id == item.id) {
        report.lines.push(format!("unlisted ID: {}", item.id));
    }
    let Some(old) = item.old_ir else {
        report.fail(format!("{}: missing or invalid base Ir for noise", item.id));
        return;
    };
    let diff = difference(item.new_ir, old);
    if exceeds(item.new_ir.abs_diff(old), old, limit) {
        report.fail(format!(
            "{}: noise {diff:.3}% exceeds {:.3}%",
            item.id,
            limit as f64 / 10.0
        ));
    }
}

/// Compare an Ir delta against a limit in permille without rounding instruction counts.
fn exceeds(delta: u64, base: u64, limit: u64) -> bool {
    u128::from(delta) * 1000 > u128::from(base) * u128::from(limit)
}

/// Compute percentage growth; equal zero counts have no growth, otherwise zero gives infinity.
fn difference(new: u64, old: u64) -> f64 {
    if new == old {
        0.0
    } else {
        (new as f64 - old as f64) / old as f64 * 100.0
    }
}

/// Start the report with its sorted instruction-count table before adding findings.
fn table(rows: &[&Measured]) -> Report {
    let mut lines = vec![
        "| ID | base `Ir` | head `Ir` | diff % |".into(),
        "| --- | ---: | ---: | ---: |".into(),
    ];
    lines.extend(rows.iter().map(|item| table_row(item)));
    lines.push(String::new());
    Report {
        lines,
        failed: false,
    }
}

/// Format an instruction-count table row, marking absent base values explicitly.
fn table_row(item: &Measured) -> String {
    let base = item
        .old_ir
        .map(|old| format!("{old:.0}"))
        .unwrap_or_else(|| "missing".into());
    let diff = item
        .old_ir
        .map(|old| format!("{:+.3}", difference(item.new_ir, old)))
        .unwrap_or_else(|| "—".into());
    format!("| {} | {base} | {:.0} | {diff} |", item.id, item.new_ir)
}

#[cfg(test)]
#[path = "verdict_tests.rs"]
mod tests;
