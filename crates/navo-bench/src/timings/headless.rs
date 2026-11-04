use super::bench_json::{BenchNumbers, parse_bench};
use super::error::TimingError;
use super::host::{Host, Subject};
use super::result::{HeadlessReport, SortKey, SortStat};
use super::run::Ids;
use crate::measure::{Metric, stat};
use crate::safe_text;

/// Each spawn of `navo bench --folder` also times five runs inside the process.
pub(super) const SPAWNS: u32 = 5;

/// Spawns the folder benchmark five times and pools the numbers of all runs.
pub(super) fn headless(
    host: &mut dyn Host,
    ids: &mut Ids,
    subject: Subject,
) -> (HeadlessReport, Option<TimingError>) {
    let mut pooled = BenchNumbers::default();
    let (mut failed, mut last) = (0, None);
    for _ in 0..SPAWNS {
        let run = ids.next();
        match host
            .run_bench(subject, run)
            .and_then(|text| parse_bench(&text))
        {
            Ok(numbers) => merge(&mut pooled, numbers),
            Err(error) => {
                failed += 1;
                last = Some(error);
            }
        }
    }
    let failure = (failed == SPAWNS).then_some(last).flatten();
    (report(subject, failed, &pooled, failure), failure)
}

fn merge(into: &mut BenchNumbers, from: BenchNumbers) {
    into.entries = into.entries.or(from.entries);
    into.bytes = into.bytes.or(from.bytes);
    into.list_ms.extend(from.list_ms);
    into.filter_keystroke_ms.extend(from.filter_keystroke_ms);
    for (slot, samples) in into.sort_ms.iter_mut().zip(from.sort_ms) {
        slot.extend(samples);
    }
}

fn report(
    subject: Subject,
    failed: u32,
    pooled: &BenchNumbers,
    failure: Option<TimingError>,
) -> HeadlessReport {
    let none = failure.map_or(
        safe_text!("build wrote no such number"),
        TimingError::reason,
    );
    let count = |value: Option<u64>| {
        value.map_or_else(|| Metric::missing(none), |value| Metric::Measured { value })
    };
    HeadlessReport {
        subject,
        attempted: SPAWNS,
        failed,
        entries: count(pooled.entries),
        top_level_bytes: count(pooled.bytes),
        list: stat(&pooled.list_ms, none),
        sorts: SortKey::ALL
            .iter()
            .zip(&pooled.sort_ms)
            .map(|(&key, samples)| SortStat {
                key,
                time: stat(samples, none),
            })
            .collect(),
        filter_keystroke: stat(&pooled.filter_keystroke_ms, none),
    }
}
