#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    reason = "ratio tests fail on broken fixtures and print the measured numbers"
)]

use super::NtfsIndex;
use super::peak_tests::{flat_index, held, hint, save, steady_index};
use super::test_support::MftFixture;
use super::volume_io::enumerate_volume_sized;
use crate::file;
use crate::heap_probe::Probe;
use crate::pipe::CancelToken;
use crate::usn::{UsnBatch, UsnRecord};

const OLD_NAME: u32 = 0x1000;
const ROOT_REFERENCE: u128 = 5;
const PEAK_CEILING: f64 = 1.7;
const REBUILD_CEILING: f64 = 2.2;
const UNSIZED_REBUILD_CEILING: f64 = 2.5;
/// First load of a snapshot an older build left up to 2x sparse; once per upgrade (owner 2026-11-05).
const SPARSE_LOAD_CEILING: f64 = 2.5;
/// A sized rebuild of a volume that grew past the hint's sixteenth (owner 2026-11-05).
const GROWN_REBUILD_CEILING: f64 = 2.6;

/// Peak over the finished index, for a build from nothing.
pub(super) fn build_ratio(entries: u32) -> (usize, usize) {
    let fixture = MftFixture::new(entries, 7);
    let probe = Probe::start();
    let index = fixture.read().index;
    let peak = probe.read().peak;
    (peak, held(index, &probe))
}

/// Peak over the finished index, for a snapshot load.
pub(super) fn load_ratio(entries: u32) -> (usize, usize) {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("c.index");
    save(&path, &flat_index(entries));
    let probe = Probe::start();
    let index = NtfsIndex::restore(file::load_with_ids(&path).unwrap(), 42, 7).unwrap();
    let peak = probe.read().peak;
    (peak, held(index, &probe))
}

/// Peak over the finished index, for a load of a snapshot with slots about 1.9 times its live entries.
pub(super) fn load_sparse_ratio(entries: u32) -> (usize, usize) {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("c.index");
    let mut sparse = flat_index(entries);
    let live = entries * 10 / 19;
    for id in live..entries {
        sparse.table.remove(id);
        sparse.ids.remove(&(u128::from(id) + 100));
    }
    let ratio = f64::from(sparse.table.slots()) / sparse.table.len() as f64;
    assert!((ratio - 1.9).abs() < 0.01, "slots/live {ratio:.3}");
    save(&path, &sparse);
    drop(sparse);
    let probe = Probe::start();
    let index = NtfsIndex::restore(file::load_with_ids(&path).unwrap(), 42, 7).unwrap();
    let peak = probe.read().peak;
    (peak, held(index, &probe))
}

/// Peak over the sparse index just before compaction starts.
pub(super) fn compact_ratio(entries: u32) -> (usize, usize) {
    let probe = Probe::start();
    let mut index = flat_index(entries);
    for id in (1..entries).step_by(2).chain([2]) {
        index.table.remove(id);
        index.ids.remove(&(u128::from(id) + 100));
    }
    let base = probe.mark();
    assert!(index.compact_if_sparse(0));
    let peak = probe.read().peak;
    drop(index);
    (peak, base)
}

/// Peak over the loaded index, for one push.
pub(super) fn push_ratio(entries: u32) -> (usize, usize) {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("c.index");
    save(&path, &flat_index(entries));
    let probe = Probe::start();
    let mut index = NtfsIndex::restore(file::load_with_ids(&path).unwrap(), 42, 7).unwrap();
    let base = probe.mark();
    index
        .table
        .push(0, "pushed.txt", crate::table::Flags::NONE)
        .unwrap();
    let peak = probe.read().peak;
    drop(index);
    (peak, base)
}

/// Peak over the old index at rest, for a rebuild that keeps the old index searchable.
pub(super) fn rebuild_ratio(entries: u32) -> (usize, usize) {
    let fixture = MftFixture::new(entries, 7);
    let probe = Probe::start();
    let old = flat_index(entries);
    let base = probe.mark();
    let rebuilt = fixture.read().index;
    let peak = probe.read().peak;
    drop(old);
    drop(rebuilt);
    (peak, base)
}

/// Like `rebuild_ratio`, but sized from an old index with `old_entries` to build `new_entries`.
pub(super) fn sized_rebuild_ratio_at(old_entries: u32, new_entries: u32) -> (usize, usize) {
    let probe = Probe::start();
    let old = flat_index(old_entries);
    let hint = hint(&old);
    let base = probe.mark();
    let rebuilt = enumerate_volume_sized(
        &mut MftFixture::new(new_entries, 7),
        'C',
        usize::MAX,
        hint,
        &CancelToken::new(),
    )
    .unwrap()
    .index;
    let peak = probe.read().peak;
    drop(old);
    drop(rebuilt);
    (peak, base)
}

/// Like `rebuild_ratio`, but sized from the old index as the service does.
pub(super) fn sized_rebuild_ratio(entries: u32) -> (usize, usize) {
    sized_rebuild_ratio_at(entries, entries)
}

/// Bytes per entry the index holds after every file is renamed once.
pub(super) fn rename_churn(entries: u32) -> f64 {
    let (mut index, probe) = steady_index(entries, 0);
    probe.mark();
    for id in (1..entries).filter(|id| id % 10 != 0) {
        let parent = index.table.parent(id).unwrap();
        index
            .table
            .rename(id, parent, &format!("renamed-{id:07}.txt"))
            .unwrap();
    }
    held(index, &probe) as f64 / f64::from(entries)
}

fn old_name_record(number: u32) -> UsnRecord {
    UsnRecord {
        file_id: u128::from(number) + 100,
        parent_id: ROOT_REFERENCE,
        usn: 100,
        reason: OLD_NAME,
        flags: crate::table::Flags::NONE,
        name: format!("entry-{number:07}.txt"),
        size: None,
    }
}

/// Peak and held growth per pending rename, after one batch of `pending` old-name records.
pub(super) fn pending_renames(entries: u32, pending: u32) -> (f64, f64) {
    let (mut index, probe) = steady_index(entries, 0);
    let before = probe.read().retained;
    let records = (1..entries)
        .filter(|number| number % 10 != 0)
        .take(pending as usize)
        .map(old_name_record)
        .collect();
    let batch = UsnBatch {
        next_usn: index.position.next_usn + 1,
        records,
    };
    let base = probe.mark();
    assert!(index.apply_batch(batch).unwrap().is_empty());
    assert_eq!(index.pending_renames().len(), pending as usize);
    let peak = probe.read().peak.saturating_sub(base);
    let grown = (probe.read().retained - before).max(0) as usize;
    drop(index);
    (
        peak as f64 / f64::from(pending),
        grown as f64 / f64::from(pending),
    )
}

fn ratio((peak, base): (usize, usize)) -> f64 {
    peak as f64 / base as f64
}

fn line(case: &str, entries: u32, (peak, base): (usize, usize)) {
    println!(
        "RATIO {case} {}: {:.3} (peak {} KiB, base {} KiB)",
        entries / 1_000_000,
        ratio((peak, base)),
        peak >> 10,
        base >> 10
    );
}

#[test]
fn every_ratio_is_finite_and_above_one() {
    let entries = 100_000;
    for measured in [
        build_ratio(entries),
        load_ratio(entries),
        load_sparse_ratio(entries),
        compact_ratio(entries),
        push_ratio(entries),
        rebuild_ratio(entries),
        sized_rebuild_ratio(entries),
    ] {
        let value = ratio(measured);
        assert!(value.is_finite() && value > 1.0, "{measured:?}");
    }
    let (peak, grown) = pending_renames(entries, 10_000);
    assert!(peak.is_finite() && grown.is_finite());
}

#[test]
#[ignore = "peak ratio report"]
fn peak_ratio_report() {
    for entries in [1_000_000, 2_000_000, 5_000_000] {
        line("build", entries, build_ratio(entries));
        line("load", entries, load_ratio(entries));
        line("load-sparse", entries, load_sparse_ratio(entries));
        line("compact", entries, compact_ratio(entries));
        line("push", entries, push_ratio(entries));
        line("rebuild", entries, rebuild_ratio(entries));
        line("rebuild-sized", entries, sized_rebuild_ratio(entries));
        line(
            "rebuild-sized-0.9x",
            entries,
            sized_rebuild_ratio_at(entries, entries * 9 / 10),
        );
        line(
            "rebuild-sized-1.1x",
            entries,
            sized_rebuild_ratio_at(entries, entries * 11 / 10),
        );
    }
    println!("CHURN rename {:.2} B/entry", rename_churn(1_000_000));
    let (peak, grown) = pending_renames(1_000_000, 100_000);
    println!("CHURN pending peak {peak:.2} B/pending held {grown:.2} B/pending");
    println!("MAP n/a");
}

/// The service sizes every rebuild from the old index (RD2); an unsized one peaks at the arena's growth.
#[test]
#[ignore = "release-only rebuild ceiling"]
fn sized_rebuild_stays_within_the_ceiling() {
    let entries = 1_000_000;
    let measured = sized_rebuild_ratio(entries);
    line("rebuild-sized", entries, measured);
    assert!(ratio(measured) <= REBUILD_CEILING, "{measured:?}");
    // A rebuild of a volume that shrank must not copy the old index's spare capacity.
    let shrunk = sized_rebuild_ratio_at(entries, entries * 9 / 10);
    line("rebuild-sized-0.9x", entries, shrunk);
    let value = ratio(shrunk);
    assert!(value <= REBUILD_CEILING, "rebuild-sized-0.9x: {value:.3}");
    let grown = sized_rebuild_ratio_at(entries, entries * 11 / 10);
    line("rebuild-sized-1.1x", entries, grown);
    let value = ratio(grown);
    assert!(
        value <= GROWN_REBUILD_CEILING,
        "rebuild-sized-1.1x: {value:.3}"
    );
}

#[test]
#[ignore = "release-only sparse load ceiling"]
fn a_sparse_snapshot_load_stays_within_the_ceiling() {
    let measured = load_sparse_ratio(1_000_000);
    line("load-sparse", 1_000_000, measured);
    let value = ratio(measured);
    assert!(value <= SPARSE_LOAD_CEILING, "load-sparse: {value:.3}");
}

#[test]
#[ignore = "release-only 5M rebuild ceiling"]
fn a_five_million_entry_rebuild_stays_within_the_ceiling() {
    let measured = rebuild_ratio(5_000_000);
    line("rebuild", 5_000_000, measured);
    let value = ratio(measured);
    assert!(value <= UNSIZED_REBUILD_CEILING, "rebuild 5M: {value:.3}");
}

#[test]
fn peaks_stay_within_d109() {
    let entries = 1_000_000;
    for (case, measured) in [
        ("build", build_ratio(entries)),
        ("load", load_ratio(entries)),
        ("compact", compact_ratio(entries)),
        ("push", push_ratio(entries)),
    ] {
        line(case, entries, measured);
        let value = ratio(measured);
        assert!(value <= PEAK_CEILING, "{case}: {value:.3}");
    }
    let measured = sized_rebuild_ratio(entries);
    line("rebuild-sized", entries, measured);
    let value = ratio(measured);
    assert!(value <= REBUILD_CEILING, "rebuild-sized: {value:.3}");
    let measured = rebuild_ratio(entries);
    line("rebuild", entries, measured);
    let value = ratio(measured);
    assert!(value <= UNSIZED_REBUILD_CEILING, "rebuild: {value:.3}");
    let churn = rename_churn(entries);
    println!("CHURN rename {churn:.2} B/entry");
    assert!(churn <= 89.0, "rename churn {churn:.2} B/entry");
}
