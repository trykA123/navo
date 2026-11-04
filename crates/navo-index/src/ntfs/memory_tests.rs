#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "memory budget tests fail on broken fixtures and print the measured numbers"
)]

use super::test_support::MftFixture;
use super::{COMPACT_MIN_SLOTS, NtfsIndex};
use crate::file::{self, Header};
use crate::heap_probe::{MIB, Measured, Probe};
use crate::table::{Flags, NO_PARENT, NameTable};
use crate::usn::FrnMap;

const ENTRIES: u32 = 1_000_000;
// Just past a power of two, where growth by doubling leaves the most unused capacity.
const GROWN_ENTRIES: u32 = 1_100_000;
// trim_slack keeps up to len / 16 spare per column, about 0.3 B per entry here.
const TABLE_BUDGET: usize = 51;
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

const LIVE_SET: u32 = 50_000;
const CHURN_OPS: u32 = 500_000;
const CHURN_BATCH: u32 = 5_000;
const CHURN_WAVES: u32 = 3;
const CHURN_ROOT: u128 = (5 << 48) | 5;

fn churn_record(file_id: u128, reason: u32, name: &str) -> crate::usn::UsnRecord {
    crate::usn::UsnRecord {
        file_id,
        parent_id: CHURN_ROOT,
        usn: 100,
        reason,
        flags: Flags::NONE,
        name: name.into(),
        size: Some(7),
    }
}

fn churn_batch(index: &NtfsIndex, records: Vec<crate::usn::UsnRecord>) -> crate::usn::UsnBatch {
    crate::usn::UsnBatch {
        next_usn: index.position.next_usn + 1,
        records,
    }
}

/// Creates, then deletes, `CHURN_BATCH` files per round through `apply_batch`, so the live set never grows.
fn churn(index: &mut NtfsIndex, operations: u32, wave: u32) {
    let mut next = 1_000_000_000u128 * u128::from(wave + 1);
    for _ in 0..operations / (2 * CHURN_BATCH) {
        let first = next;
        let created = (0..CHURN_BATCH)
            .map(|n| {
                churn_record(
                    first + u128::from(n),
                    0x100,
                    &format!("tmp-{}.txt", first + u128::from(n)),
                )
            })
            .collect();
        next += u128::from(CHURN_BATCH);
        let batch = churn_batch(index, created);
        index.apply_batch(batch).unwrap();
        let removed = (0..CHURN_BATCH)
            .map(|n| churn_record(first + u128::from(n), 0x200, "tmp.txt"))
            .collect();
        let batch = churn_batch(index, removed);
        index.apply_batch(batch).unwrap();
    }
}

#[test]
fn memory_after_churn_returns_near_the_live_set_size() {
    let mut index = NtfsIndex::restore(
        {
            let mut table = NameTable::new();
            let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
            let mut ids = FrnMap::new();
            ids.insert(5, root);
            for number in 1..LIVE_SET {
                let id = table
                    .push(root, &format!("entry-{number:07}.txt"), Flags::NONE)
                    .unwrap();
                ids.insert(u128::from(number) + 100, id);
            }
            table.shrink_to_fit();
            (header(), table, ids)
        },
        42,
        7,
    )
    .unwrap();
    // Between compactions a table holds up to twice its live set, plus one batch, plus the floor.
    let slot_bound = (2 * LIVE_SET + CHURN_BATCH + COMPACT_MIN_SLOTS) as usize;
    for wave in 0..CHURN_WAVES {
        churn(&mut index, CHURN_OPS, wave);
        let (slots, live) = (index.table.slots() as usize, index.table.len());
        let (table, ids) = (index.table.memory_bytes(), index.ids.memory_bytes());
        println!("IX11 wave {wave}: slots {slots}, live {live}, table {table} B, ids {ids} B");
        assert_eq!(live, LIVE_SET as usize);
        assert_eq!(index.ids.len(), live);
        assert!(
            slots <= slot_bound,
            "wave {wave}: {slots} slots for {live} live"
        );
        assert!(
            table <= slot_bound * TABLE_BUDGET,
            "wave {wave}: table {table} bytes"
        );
        assert!(
            ids <= slot_bound * FILE_ID_BUDGET,
            "wave {wave}: ids {ids} bytes"
        );
    }
}
