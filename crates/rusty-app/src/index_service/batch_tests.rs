use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use rusty_index::file::Header;
use rusty_index::ntfs::{NtfsError, NtfsIndex};
use rusty_index::table::{Flags, NO_PARENT, NameTable};
use rusty_index::usn::{FrnMap, UsnBatch, UsnRecord};

use super::super::answer::{Shared, State};
use super::{apply_batch, published};

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
        .map(|(&file, &id)| (file, index.table.path(id), index.table.size(id)))
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
            .filter(|&(&file, _)| file >= 100)
            .all(|(_, &id)| index.table.size(id) == Some(7))
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
fn a_failed_batch_returns_the_error_and_keeps_the_checkpoint() {
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
    let failed = apply_batch(&shared, batch);
    assert!(matches!(failed, Err(NtfsError::Table(_))), "{failed:?}");
    let state = shared.read().unwrap();
    let index = state.index.as_ref().unwrap();
    assert_eq!(index.position.next_usn, 100);
    assert!(index.ids.values().all(|&id| index.table.name(id).is_some()));
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
        Err(NtfsError::InvalidResponse(_))
    ));
    assert_eq!(names(&published(&shared).unwrap()), before);
}

#[test]
fn a_batch_without_an_index_changes_nothing() {
    let shared = shared(None);
    assert!(apply_batch(&shared, creates()).unwrap().is_empty());
    assert!(published(&shared).is_none());
}
