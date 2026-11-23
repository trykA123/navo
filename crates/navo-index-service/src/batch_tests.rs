use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use navo_index::file::Header;
use navo_index::ntfs::{NtfsError, NtfsIndex, UnresolvedChange};
use navo_index::table::{Flags, NO_PARENT, NameTable, TableChange};
use navo_index::usn::{FrnMap, UsnBatch, UsnRecord};

use super::super::answer::{Shared, State};
use super::{apply_batch, note_metadata_folders, published};

const FILES: u32 = 20_000;

fn index() -> NtfsIndex {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let folder = table.push(root, "folder", Flags::FOLDER).unwrap();
    let ids: FrnMap = [(5, root), (6, folder)].into();
    let header = Header {
        volume_serial: 42,
        journal_id: 7,
        next_usn: 100,
        root: "C:".into(),
        built_at: 0,
    };
    NtfsIndex::restore((header, table, ids), 42, 7).unwrap()
}

fn shared(index: Option<NtfsIndex>) -> Shared {
    Arc::new(RwLock::new(State {
        index: index.map(Arc::new),
        ..State::default()
    }))
}

fn record(file_id: u128, parent_id: u128, reason: u32, name: &str) -> UsnRecord {
    UsnRecord {
        file_id,
        parent_id,
        usn: 100,
        reason,
        flags: Flags::NONE,
        name: name.into(),
        size: Some(7),
    }
}

fn creates() -> UsnBatch {
    UsnBatch {
        next_usn: 200,
        records: (0..FILES)
            .map(|number| {
                record(
                    u128::from(number) + 100,
                    5,
                    0x100,
                    &format!("f{number}.txt"),
                )
            })
            .collect(),
    }
}

fn names(index: &NtfsIndex) -> Vec<(u128, Option<String>, Option<u64>)> {
    let mut rows: Vec<_> = index
        .ids
        .iter()
        .map(|(file, id)| (file, index.table.path(id), index.table.size(id)))
        .collect();
    rows.sort();
    rows
}

fn partial_reads(shared: &Shared, done: &AtomicBool) -> usize {
    let mut partial = 0;
    while !done.load(Ordering::Acquire) {
        if let Ok(state) = shared.try_read() {
            partial += usize::from(whole_records_only(state.index.as_ref().unwrap()));
        }
        std::thread::sleep(std::time::Duration::from_micros(500));
    }
    partial
}

fn whole_records_only(index: &NtfsIndex) -> bool {
    let live = index.table.len();
    if live <= 2 || live >= FILES as usize + 2 {
        return false;
    }
    assert_eq!(index.position.next_usn, 100);
    assert!(
        index
            .ids
            .iter()
            .filter(|&(file, _)| file >= 100)
            .all(|(_, id)| index.table.size(id) == Some(7))
    );
    true
}

#[test]
fn a_large_batch_lets_readers_in_between_steps_and_matches_one_batch() {
    let shared = shared(Some(index()));
    let mut expected = index();
    expected.apply_batch(creates()).unwrap();
    let done = AtomicBool::new(false);
    let partial = std::thread::scope(|scope| {
        let reader = scope.spawn(|| partial_reads(&shared, &done));
        let applied = apply_batch(&shared, creates());
        done.store(true, Ordering::Release);
        assert!(applied.unwrap().is_empty());
        reader.join().unwrap()
    });
    assert!(partial > 0, "no reader got in during the batch");
    let state = shared.read().unwrap();
    let index = state.index.as_ref().unwrap();
    assert_eq!(names(index), names(&expected));
    assert_eq!(index.position, expected.position);
    assert_eq!(index.table.totals(0), expected.table.totals(0));
}

#[test]
fn a_saved_copy_stays_whole_while_a_batch_applies() {
    let shared = shared(Some(index()));
    let saved = published(&shared).unwrap();
    let before = names(&saved);
    apply_batch(&shared, creates()).unwrap();
    assert_eq!(names(&saved), before);
    assert_eq!(saved.position.next_usn, 100);
    assert!(saved.snapshot().is_some());
    let state = shared.read().unwrap();
    assert_eq!(
        state.index.as_ref().unwrap().table.len(),
        FILES as usize + 2
    );
}

#[test]
fn a_batch_with_a_cyclic_move_keeps_applying_changes() {
    let shared = shared(Some(index()));
    let mut batch = creates();
    let folder = |file_id, parent_id, reason, name| UsnRecord {
        flags: Flags::FOLDER,
        size: None,
        ..record(file_id, parent_id, reason, name)
    };
    batch.records.push(folder(50, 5, 0x100, "a"));
    batch.records.push(folder(51, 50, 0x100, "b"));
    batch.records.push(folder(50, 51, 0x2000, "a"));
    batch.records.push(folder(52, 50, 0x100, "c"));
    assert!(apply_batch(&shared, batch).unwrap().is_empty());
    let state = shared.read().unwrap();
    let index = state.index.as_ref().unwrap();
    assert_eq!(index.position.next_usn, 200);
    assert_eq!(index.table.parent(index.ids[&52]), Some(index.ids[&50]));
    assert!(index.ids.values().all(|&id| index.table.name(id).is_some()));
}

/// Counts a stale move against the newer index and keeps the published folder hierarchy.
#[test]
fn a_stale_replayed_move_that_would_cycle_is_skipped() {
    let mut newer = index();
    let child = newer
        .table
        .push(newer.ids[&6], "child", Flags::FOLDER)
        .unwrap();
    newer.ids.insert(7, child);
    let shared = shared(Some(newer));
    let folder = |parent_id, reason| UsnRecord {
        flags: Flags::FOLDER,
        size: None,
        ..record(6, parent_id, reason, "folder")
    };
    let batch = UsnBatch {
        next_usn: 120,
        records: vec![folder(5, 0x1000), folder(7, 0x2000)],
    };
    assert!(apply_batch(&shared, batch).unwrap().is_empty());
    let state = shared.read().unwrap();
    let index = state.index.as_ref().unwrap();
    assert_eq!(index.table.parent(index.ids[&6]), Some(index.ids[&5]));
    assert_eq!(index.table.parent(index.ids[&7]), Some(index.ids[&6]));
    assert_eq!(index.position.next_usn, 120);
    assert_eq!(state.builds.dropped_changes, 1);
}

/// Flags the replay when a stale hierarchy causes the only move of A to be skipped.
#[test]
fn a_cycle_skipped_replay_move_marks_follow_inexact() {
    let mut newer = index();
    let a = newer.ids[&6];
    let c = newer.table.push(a, "C", Flags::FOLDER).unwrap();
    let b = newer.table.push(c, "B", Flags::FOLDER).unwrap();
    newer.ids.insert(7, b);
    newer.ids.insert(8, c);
    let shared = shared(Some(newer));
    let folder = |file_id, parent_id, reason, name| UsnRecord {
        flags: Flags::FOLDER,
        size: None,
        ..record(file_id, parent_id, reason, name)
    };
    let batch = UsnBatch {
        next_usn: 120,
        records: vec![
            folder(6, 5, 0x1000, "A"),
            folder(6, 7, 0x2000, "A"),
            folder(8, 6, 0x1000, "C"),
            folder(8, 5, 0x2000, "C"),
            folder(7, 5, 0x1000, "B"),
            folder(7, 8, 0x2000, "B"),
        ],
    };
    assert!(apply_batch(&shared, batch).unwrap().is_empty());
    let state = shared.read().unwrap();
    assert!(state.replay_inexact, "the stale move made follow inexact");
    assert_eq!(state.builds.dropped_changes, 1);
    let index = state.index.as_ref().unwrap();
    assert_eq!(index.table.parent(a), Some(0));
    assert_eq!(index.table.parent(c), Some(0));
    assert_eq!(index.position.next_usn, 120);
}

#[test]
fn a_stale_cursor_changes_nothing() {
    let shared = shared(Some(index()));
    let before = names(&published(&shared).unwrap());
    let stale = UsnBatch {
        next_usn: 90,
        ..creates()
    };
    assert!(matches!(
        apply_batch(&shared, stale),
        Err(NtfsError::Reenumerate("cursor"))
    ));
    assert_eq!(names(&published(&shared).unwrap()), before);
}

#[test]
fn a_batch_without_an_index_changes_nothing() {
    let shared = shared(None);
    assert!(apply_batch(&shared, creates()).unwrap().is_empty());
    assert!(published(&shared).is_none());
}

#[test]
fn a_batch_after_the_reader_is_dropped_updates_in_place() {
    let shared = shared(Some(index()));
    let reader = published(&shared).unwrap();
    let before = Arc::as_ptr(&reader);
    drop(reader);
    apply_batch(&shared, creates()).unwrap();
    let after = published(&shared).unwrap();
    assert_eq!(Arc::as_ptr(&after), before);
    assert_eq!(after.ids.len(), 2 + FILES as usize);
}

fn churn_files(count: u32, reason: u32, next_usn: i64) -> UsnBatch {
    UsnBatch {
        next_usn,
        records: (0..count)
            .map(|number| {
                record(
                    u128::from(number) + 100,
                    6,
                    reason,
                    &format!("c{number}.txt"),
                )
            })
            .collect(),
    }
}

#[test]
fn ids_a_reader_took_before_a_compacting_batch_still_name_the_same_files() {
    let files = navo_index::ntfs::COMPACT_MIN_SLOTS + 1_000;
    let shared = shared(Some(index()));
    assert!(
        apply_batch(&shared, churn_files(files, 0x100, 200))
            .unwrap()
            .is_empty()
    );
    let reader = published(&shared).unwrap();
    let kept = u128::from(files - 1) + 100;
    let held = reader.ids[&kept];
    let path = reader.table.path(held);
    assert!(
        path.as_deref()
            .is_some_and(|path| path.ends_with(&format!("c{}.txt", files - 1)))
    );
    apply_batch(&shared, churn_files(files - 10, 0x200, 300)).unwrap();
    let after = published(&shared).unwrap();
    assert_eq!(
        after.table.slots() as usize,
        after.table.len(),
        "the batch compacted"
    );
    assert_ne!(after.ids[&kept], held);
    assert_eq!(reader.table.path(held), path);
    assert_eq!(after.table.path(after.ids[&kept]), path);
}

/// AF9e: after a restore the index has no metadata-folder set. Once the follower notes
/// `$Extend\$Deleted` (confirmed from the MFT), a file Windows parks there leaves the index and
/// the batch reports no named change, so no rebuild is asked for.
#[test]
fn a_noted_metadata_folder_resolves_a_rename_into_it_after_a_restore() {
    const DELETED: u128 = (1 << 48) | 0x1D;
    let parked = || UsnBatch {
        next_usn: 130,
        records: vec![
            record(8, 6, 0x1000, "x.txt"),
            record(8, DELETED, 0x2000, "$X8"),
        ],
    };
    let create = UsnBatch {
        next_usn: 120,
        records: vec![record(8, 6, 0x100, "x.txt")],
    };
    let blocked = shared(Some(index()));
    assert!(apply_batch(&blocked, create.clone()).unwrap().is_empty());
    assert!(
        apply_batch(&blocked, parked())
            .unwrap()
            .iter()
            .any(|item| matches!(item.change, TableChange::Rename { .. }))
    );

    let shared = shared(Some(index()));
    assert!(apply_batch(&shared, create).unwrap().is_empty());
    note_metadata_folders(&shared, vec![DELETED]);
    let unresolved = apply_batch(&shared, parked()).unwrap();
    // The size of the departed file has no entry left; sizes alone never ask for a rebuild.
    let named = |item: &UnresolvedChange| {
        matches!(
            item.change,
            TableChange::Add { .. } | TableChange::Rename { .. }
        )
    };
    assert!(!unresolved.iter().any(named), "{unresolved:?}");
    let state = shared.read().unwrap();
    assert!(state.index.as_ref().unwrap().ids.get(&8).is_none());
}
