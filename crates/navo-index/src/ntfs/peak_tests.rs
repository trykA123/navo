#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "peak tests fail on broken fixtures and print the measured numbers"
)]

use super::test_support::MftFixture;
use super::volume_io::enumerate_volume_sized;
use super::{COMPACT_MIN_SLOTS, NtfsIndex, SizeHint};
use crate::file::{self, Header};
use crate::heap_probe::{Measured, Probe};
use crate::pipe::CancelToken;
use crate::table::{Flags, NO_PARENT, NameTable};
use crate::usn::FrnMap;

const ENTRIES: u32 = 1_000_000;
// Peak budgets: the value measured on Linux at this size, plus 10 per cent; steady plus 2 per cent.
// Just past a power of two, where growth by doubling leaves the most unused capacity.
const GROWN_ENTRIES: u32 = 1_100_000;
const SCALES: [u32; 2] = [2_000_000, 5_000_000];
const BUILD_EXTRA: usize = 15;
const BUILD_SIZED_EXTRA: usize = 1;
const COMPACT_EXTRA: usize = 32;
const LOAD_EXTRA: usize = 9;
const LOAD_FOLDERS_EXTRA: usize = 13;
const PUSH_EXTRA: usize = 22;
const STEADY_PER_ENTRY: usize = 52;

#[test]
fn steady_stays_within_budget() {
    let (index, probe) = steady_index(ENTRIES, 0);
    let columns: usize = index
        .table
        .column_bytes()
        .iter()
        .map(|(_, bytes)| bytes)
        .sum();
    let accounted = columns + index.ids.memory_bytes();
    let marked = probe.mark();
    let retained = held(index, &probe);
    assert!(retained <= STEADY_PER_ENTRY * ENTRIES as usize);
    assert!(accounted.abs_diff(retained) <= retained / 100);
    assert!(marked.abs_diff(retained) <= retained / 100);
}

pub(super) fn header() -> Header {
    Header {
        volume_serial: 42,
        journal_id: 7,
        next_usn: 100,
        root: "C:".into(),
        built_at: 0,
    }
}

/// Heap bytes the index holds, measured by dropping it.
pub(super) fn held(index: NtfsIndex, probe: &Probe) -> usize {
    let before = probe.read().retained;
    drop(index);
    (before - probe.read().retained).unsigned_abs()
}

fn report(case: &str, entries: usize, measured: Measured, per_entry: usize) {
    println!(
        "RAM {case}: peak {} KiB, retained {} KiB, per-entry {per_entry} B ({entries} entries)",
        measured.peak >> 10,
        measured.retained >> 10,
    );
}

pub(super) fn save(path: &std::path::Path, index: &NtfsIndex) {
    let (table, ids, position) = index.snapshot().unwrap();
    let header = Header {
        next_usn: position.next_usn,
        ..header()
    };
    file::save_with_ids(path, &header, table, ids).unwrap();
}

/// A root and `entries - 1` files, each file reference one hundred past its id.
pub(super) fn flat_index(entries: u32) -> NtfsIndex {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let mut ids = FrnMap::with_capacity(entries as usize);
    ids.insert(5, root);
    for number in 1..entries {
        let id = table
            .push(root, &format!("entry-{number:07}.txt"), Flags::NONE)
            .unwrap();
        ids.insert(u128::from(number) + 100, id);
    }
    table.shrink_to_fit();
    NtfsIndex::restore((header(), table, ids), 42, 7).unwrap()
}

pub(super) fn steady_index(entries: u32, record_gap: u32) -> (NtfsIndex, Probe) {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("steady.index");
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let mut ids = FrnMap::with_capacity(entries as usize);
    ids.insert(5, root);
    let mut parent = root;
    for number in 1..entries {
        let flags = if number % 10 == 0 {
            Flags::FOLDER
        } else {
            Flags::NONE
        };
        let id = table
            .push(
                if flags == Flags::FOLDER { root } else { parent },
                &format!("entry-{number:07}.txt"),
                flags,
            )
            .unwrap();
        if flags == Flags::FOLDER {
            parent = id;
        }
        let gap = number.checked_div(record_gap).unwrap_or(0);
        ids.insert(u128::from(100 + number + gap), id);
    }
    let source = NtfsIndex::restore((header(), table, ids), 42, 7).unwrap();
    save(&path, &source);
    drop(source);
    let probe = Probe::start();
    let index = NtfsIndex::restore(file::load_with_ids(&path).unwrap(), 42, 7).unwrap();
    (index, probe)
}

#[test]
#[ignore = "steady report"]
fn steady_report() {
    for entries in [1_000_000, 2_000_000, 5_000_000] {
        for gap in [0, 4] {
            let (index, probe) = steady_index(entries, gap);
            let columns = index.table.column_bytes();
            let frn = index.ids.memory_bytes();
            let total: usize = columns.iter().map(|(_, bytes)| bytes).sum::<usize>() + frn;
            let retained = held(index, &probe);
            print!("STEADY {} gap {}:", entries / 1_000_000, gap);
            for (name, bytes) in columns {
                print!(" {name} {:.2}", bytes as f64 / f64::from(entries));
            }
            println!(
                " frn {:.2} total {:.2} held {} KiB",
                frn as f64 / f64::from(entries),
                total as f64 / f64::from(entries),
                retained >> 10
            );
            assert!(total.abs_diff(retained) <= retained / 100);
        }
    }
}

/// A root, one folder per fourth entry, and the other entries as files inside the last folder.
fn folder_index(entries: u32) -> NtfsIndex {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let mut ids = FrnMap::with_capacity(entries as usize);
    ids.insert(5, root);
    let mut folder = root;
    for number in 1..entries {
        let id = if number % 4 == 0 {
            folder = table
                .push(root, &format!("folder-{number:07}"), Flags::FOLDER)
                .unwrap();
            folder
        } else {
            table
                .push(folder, &format!("file-{number:07}.txt"), Flags::NONE)
                .unwrap()
        };
        ids.insert(u128::from(number) + 100, id);
    }
    table.shrink_to_fit();
    NtfsIndex::restore((header(), table, ids), 42, 7).unwrap()
}

fn build_peak(entries: u32) -> (Measured, usize) {
    let fixture = MftFixture::new(entries, 7);
    let probe = Probe::start();
    let index = fixture.read().index;
    let measured = probe.read();
    let retained = held(index, &probe);
    (measured, retained)
}

/// Total capacities a previous build suggests, with the one-sixteenth slack.
pub(super) fn hint(index: &NtfsIndex) -> SizeHint {
    let entries = index.table.len();
    let folders = index.table.folder_count();
    let names = index.table.live_name_bytes();
    let entries = entries + entries / 16;
    SizeHint {
        entries,
        folders: (folders + folders / 16).min(entries),
        name_bytes: names + names / 16,
    }
}

fn sized_build_peak(entries: u32) -> (Measured, usize) {
    let first = MftFixture::new(entries, 7).read();
    let hint = hint(&first.index);
    drop(first);
    let probe = Probe::start();
    let result = enumerate_volume_sized(
        &mut MftFixture::new(entries, 7),
        'C',
        usize::MAX,
        hint,
        &CancelToken::new(),
    )
    .unwrap();
    let measured = probe.read();
    let retained = held(result.index, &probe);
    (measured, retained)
}

fn load_peak(entries: u32, folders: bool) -> (Measured, usize) {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("c.index");
    let source = if folders {
        folder_index(entries)
    } else {
        flat_index(entries)
    };
    save(&path, &source);
    drop(source);
    let probe = Probe::start();
    let loaded = file::load_with_ids(&path).unwrap();
    let index = NtfsIndex::restore(loaded, 42, 7).unwrap();
    let measured = probe.read();
    let retained = held(index, &probe);
    (measured, retained)
}

fn compact_peak(entries: u32) -> (Measured, usize) {
    let mut index = flat_index(entries);
    for id in (1..entries).step_by(2) {
        index.table.remove(id);
        index.ids.remove(&(u128::from(id) + 100));
    }
    index.table.remove(2);
    index.ids.remove(&(u128::from(2u32) + 100));
    assert!(index.table.slots() > 2 * index.table.len() as u32);
    let probe = Probe::start();
    assert!(index.compact_if_sparse(COMPACT_MIN_SLOTS));
    let measured = probe.read();
    let live = index.table.len();
    assert_eq!(index.table.slots() as usize, live);
    let retained = held(index, &probe);
    (measured, retained)
}

fn push_peak(entries: u32) -> Measured {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("c.index");
    save(&path, &flat_index(entries));
    let loaded = file::load_with_ids(&path).unwrap();
    let mut index = NtfsIndex::restore(loaded, 42, 7).unwrap();
    let probe = Probe::start();
    index.table.push(0, "pushed.txt", Flags::NONE).unwrap();
    probe.read()
}

#[test]
fn build_stays_within_its_peak() {
    let (measured, index) = build_peak(GROWN_ENTRIES);
    let entries = GROWN_ENTRIES as usize;
    let per_entry = measured.peak.saturating_sub(index) / entries;
    report("build", entries, measured, per_entry);
    assert!(per_entry <= BUILD_EXTRA, "build {per_entry} B/entry");
}

#[test]
fn sized_build_stays_within_its_peak() {
    let (measured, index) = sized_build_peak(GROWN_ENTRIES);
    let entries = GROWN_ENTRIES as usize;
    let per_entry = measured.peak.saturating_sub(index) / entries;
    report("build_sized", entries, measured, per_entry);
    assert!(
        per_entry <= BUILD_SIZED_EXTRA,
        "build_sized {per_entry} B/entry"
    );
}

#[test]
fn compact_stays_within_its_peak() {
    let (measured, index) = compact_peak(ENTRIES);
    let live = ENTRIES as usize / 2 - 1;
    let per_entry = measured.peak / live;
    report("compact", live, measured, per_entry);
    assert!(per_entry <= COMPACT_EXTRA, "compact {per_entry} B/entry");
    let _ = index;
}

#[test]
fn load_stays_within_its_peak() {
    let (measured, index) = load_peak(ENTRIES, false);
    let entries = ENTRIES as usize;
    let per_entry = measured.peak.saturating_sub(index) / entries;
    report("load", entries, measured, per_entry);
    assert!(per_entry <= LOAD_EXTRA, "load {per_entry} B/entry");
}

#[test]
fn folder_load_stays_within_its_peak() {
    let (measured, index) = load_peak(ENTRIES, true);
    let entries = ENTRIES as usize;
    let per_entry = measured.peak.saturating_sub(index) / entries;
    report("load_folders", entries, measured, per_entry);
    assert!(
        per_entry <= LOAD_FOLDERS_EXTRA,
        "load_folders {per_entry} B/entry"
    );
}

#[test]
fn push_after_load_stays_within_its_peak() {
    let measured = push_peak(ENTRIES);
    let entries = ENTRIES as usize;
    let per_entry = measured.peak / entries;
    report("push_after_load", entries, measured, per_entry);
    assert!(
        per_entry <= PUSH_EXTRA,
        "push_after_load {per_entry} B/entry"
    );
}

#[test]
#[ignore = "scale report"]
fn scale_report() {
    for entries in SCALES {
        let label = format!("build {}", entries / 1_000_000);
        let (measured, index) = build_peak(entries);
        report(
            &label,
            entries as usize,
            measured,
            measured.peak.saturating_sub(index) / entries as usize,
        );
        let label = format!("build_sized {}", entries / 1_000_000);
        let (measured, index) = sized_build_peak(entries);
        report(
            &label,
            entries as usize,
            measured,
            measured.peak.saturating_sub(index) / entries as usize,
        );
        let label = format!("compact {}", entries / 1_000_000);
        let (measured, index) = compact_peak(entries);
        report(
            &label,
            entries as usize / 2 - 1,
            measured,
            measured.peak / (entries as usize / 2 - 1),
        );
        let _ = index;
        for (case, folders) in [("load", false), ("load_folders", true)] {
            let label = format!("{case} {}", entries / 1_000_000);
            let (measured, index) = load_peak(entries, folders);
            report(
                &label,
                entries as usize,
                measured,
                measured.peak.saturating_sub(index) / entries as usize,
            );
        }
        let label = format!("push_after_load {}", entries / 1_000_000);
        let measured = push_peak(entries);
        report(
            &label,
            entries as usize,
            measured,
            measured.peak / entries as usize,
        );
    }
}
