#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "memory budget tests fail on broken fixtures and print the measured numbers"
)]

use super::NtfsIndex;
use super::test_support::MftFixture;
use crate::file::{self, Header};
use crate::heap_probe::{MIB, Measured, Probe};
use crate::table::{Flags, NO_PARENT, NameTable};
use crate::usn::FrnMap;

const ENTRIES: u32 = 1_000_000;
// Just past a power of two, where growth by doubling leaves the most unused capacity.
const GROWN_ENTRIES: u32 = 1_100_000;
const TABLE_BUDGET: usize = 50;
const FILE_ID_BUDGET: usize = 40;
const STREAM_SLACK: usize = MIB;
// Restore checks rebuild per-entry counts beside the table; the old full body copy cost about 61.
const LOAD_CHECK_BUDGET: usize = 24;

fn header() -> Header {
    Header {
        volume_serial: 42,
        journal_id: 7,
        next_usn: 100,
        root: "C:".into(),
        built_at: 0,
    }
}

fn synthetic_index() -> NtfsIndex {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let mut ids = FrnMap::with_capacity(ENTRIES as usize);
    ids.insert(5, root);
    for number in 1..ENTRIES {
        let id = table
            .push(root, &format!("entry-{number:07}.txt"), Flags::NONE)
            .unwrap();
        ids.insert(u128::from(number) + 100, id);
    }
    table.shrink_to_fit();
    NtfsIndex::restore((header(), table, ids), 42, 7).unwrap()
}

/// Heap bytes the index holds, measured by dropping it.
fn held(index: NtfsIndex, probe: &Probe) -> usize {
    let before = probe.read().retained;
    drop(index);
    (before - probe.read().retained).unsigned_abs()
}

fn report(case: &str, entries: u32, measured: Measured, index: usize) {
    println!(
        "IX6 {case}: peak {} KiB, retained {} KiB, index {} KiB ({entries} entries)",
        measured.peak >> 10,
        measured.retained >> 10,
        index >> 10
    );
}

fn save(path: &std::path::Path, index: &NtfsIndex) {
    let (table, ids, position) = index.snapshot().unwrap();
    let header = Header {
        next_usn: position.next_usn,
        ..header()
    };
    file::save_with_ids(path, &header, table, ids).unwrap();
}

#[test]
fn a_first_build_keeps_no_growth_slack() {
    let fixture = MftFixture::new(GROWN_ENTRIES, 7);
    let probe = Probe::start();
    let index = fixture.read().index;
    let measured = probe.read();
    let entries = GROWN_ENTRIES as usize;
    report(
        "first build",
        GROWN_ENTRIES,
        measured,
        held(index.clone(), &probe),
    );
    assert!(index.table.memory_bytes() <= entries * TABLE_BUDGET);
    assert!(index.ids.memory_bytes() <= entries * FILE_ID_BUDGET);
}

#[test]
fn file_ids_stay_within_their_budget() {
    let index = synthetic_index();
    let entries = ENTRIES as usize;
    assert!(
        index.ids.memory_bytes() <= entries * FILE_ID_BUDGET,
        "{} bytes for {entries} references",
        index.ids.memory_bytes()
    );
}

#[test]
fn saving_never_holds_a_copy_of_the_snapshot() {
    let index = synthetic_index();
    let folder = tempfile::tempdir().unwrap();
    let probe = Probe::start();
    save(&folder.path().join("c.index"), &index);
    let measured = probe.read();
    report("save", ENTRIES, measured, held(index, &probe));
    assert!(
        measured.peak <= STREAM_SLACK,
        "save peak {} bytes",
        measured.peak
    );
}

#[test]
fn loading_holds_only_the_loaded_index() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("c.index");
    save(&path, &synthetic_index());
    let probe = Probe::start();
    let loaded = file::load_with_ids(&path).unwrap();
    let index = NtfsIndex::restore(loaded, 42, 7).unwrap();
    let measured = probe.read();
    let index_bytes = held(index, &probe);
    report("load", ENTRIES, measured, index_bytes);
    assert!(measured.peak <= index_bytes + ENTRIES as usize * LOAD_CHECK_BUDGET);
}
