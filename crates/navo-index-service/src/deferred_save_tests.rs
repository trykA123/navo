//! AF9d: while a full rebuild waits for unresolved changes, a due save writes the cursor from
//! before them, so a restore replays them and ends with the live index's entries.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use navo_index::file::{self, Header};
use navo_index::ntfs::{NtfsError, NtfsIndex, UnresolvedChange};
use navo_index::pipe::BuildReason;
use navo_index::table::{Flags, NO_PARENT, NameTable};
use navo_index::usn::{FrnMap, JournalPosition, UsnBatch, UsnRecord};

use super::answer::{Shared, State};
use super::batch::apply_batch;
use super::config::Drive;
use super::lifecycle::SaveClock;
use super::rebuild::{Cause, Followed, Rebuilds, SaveCursor};
use super::rebuild_session::{SaveInput, save_snapshot};
use super::service_log::Log;

const MINUTE: Duration = Duration::from_secs(60);
const CREATE: u32 = 0x100;
const DELETE: u32 = 0x200;
const OLD_NAME: u32 = 0x1000;
const NEW_NAME: u32 = 0x2000;
const START: JournalPosition = JournalPosition {
    journal_id: 7,
    next_usn: 100,
};

fn header(next_usn: i64) -> Header {
    Header {
        volume_serial: 42,
        journal_id: 7,
        next_usn,
        root: "C:".into(),
        built_at: 0,
    }
}

/// `C:\docs\a.txt` (folder 10, file 11) at journal 7, USN 100.
fn shared() -> Shared {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let docs = table.push(root, "docs", Flags::FOLDER).unwrap();
    let file = table.push(docs, "a.txt", Flags::NONE).unwrap();
    let ids: FrnMap = [(5, root), (10, docs), (11, file)].into();
    let index = NtfsIndex::restore((header(100), table, ids), 42, 7).unwrap();
    Arc::new(RwLock::new(State {
        index: Some(Arc::new(index)),
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
        size: (reason == CREATE).then_some(3),
    }
}

/// The journal after USN 100: a create under unknown folder 777 that cannot resolve beside one
/// that can, then a rename of `a.txt` and the delete of the new file.
fn journal() -> [UsnBatch; 2] {
    [
        UsnBatch {
            next_usn: 200,
            records: vec![
                record(20, 777, CREATE, "lost.txt"),
                record(21, 10, CREATE, "new.txt"),
            ],
        },
        UsnBatch {
            next_usn: 300,
            records: vec![
                record(11, 10, OLD_NAME, "a.txt"),
                record(11, 10, NEW_NAME, "b.txt"),
                record(21, 10, DELETE, "new.txt"),
            ],
        },
    ]
}

/// Lists every live entry as (file reference, name, parent name), sorted.
fn entries(index: &NtfsIndex) -> Vec<(u128, String, Option<String>)> {
    let table = &index.table;
    let mut entries: Vec<_> = index
        .ids
        .iter()
        .filter_map(|(file_id, id)| {
            let name = table.name(id)?.to_owned();
            let parent = table.parent(id).and_then(|parent| table.name(parent));
            Some((file_id, name, parent.map(str::to_owned)))
        })
        .collect();
    entries.sort();
    entries
}

fn drive() -> Drive {
    Drive {
        letter: 'C',
        serial: 42,
        fixed: true,
        ntfs: true,
    }
}

/// Saves the published index at `cursor` as a stopping save; returns the entries written.
fn save(shared: &Shared, path: &std::path::Path, cursor: SaveCursor) -> Option<usize> {
    let mut clock = SaveClock::default();
    clock.changed(Duration::ZERO);
    let input = SaveInput {
        shared,
        drive: drive(),
        path,
        elapsed: MINUTE,
        stopping: true,
        built_at: 0,
        cursor,
    };
    save_snapshot(input, &mut clock).unwrap()
}

/// A rebuild ran at minute 0, so the first unresolved batch after it waits for the window.
fn rebuilds_after_a_build(shared: &Shared) -> Rebuilds {
    let mut rebuilds = Rebuilds::new('C', Log::default());
    rebuilds.started(
        shared,
        Cause::new(BuildReason::SnapshotMissing),
        Duration::ZERO,
    );
    rebuilds
}

fn follow(shared: &Shared, rebuilds: &mut Rebuilds, batch: UsnBatch, start: bool) {
    let start = start.then(|| shared.read().unwrap().index.as_ref().unwrap().position);
    let followed = Followed {
        changes: apply_batch(shared, batch),
        start,
    };
    let cause = rebuilds.after_follow_from(shared, followed, MINUTE);
    assert_eq!(cause.unwrap(), None, "the rebuild waits for its window");
}

fn missing(unresolved: &[UnresolvedChange]) -> Vec<u128> {
    unresolved
        .iter()
        .map(|item| item.missing_reference)
        .collect()
}

#[test]
fn a_deferred_rebuild_saves_before_the_unresolved_batch_and_a_restore_replays_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("volume.index");
    let shared = shared();
    let mut rebuilds = rebuilds_after_a_build(&shared);
    let [first, second] = journal();
    follow(&shared, &mut rebuilds, first, true);
    follow(&shared, &mut rebuilds, second, true);
    assert!(rebuilds.is_deferred());
    assert_eq!(rebuilds.save_cursor(), SaveCursor::From(START));
    let live = entries(shared.read().unwrap().index.as_ref().unwrap());
    assert_eq!(
        save(&shared, &path, rebuilds.save_cursor()),
        Some(live.len())
    );

    let (saved, table, ids) = file::load_with_ids(&path).unwrap();
    assert_eq!(
        saved.next_usn, 100,
        "the cursor stays before the unresolved batch"
    );
    let mut restored = NtfsIndex::restore((saved, table, ids), 42, 7).unwrap();
    let [first, second] = journal();
    let replayed = restored.apply_batch(first).unwrap();
    let refs = missing(&replayed);
    assert!(
        !refs.is_empty() && refs.iter().all(|&reference| reference == 777),
        "{refs:?}"
    );
    assert!(restored.apply_batch(second).unwrap().is_empty());
    assert_eq!(entries(&restored), live);
    assert_eq!(restored.position.next_usn, 300);
}

#[test]
fn saves_skip_without_a_safe_cursor_and_resume_once_an_index_is_published() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("volume.index");
    let shared = shared();
    let mut rebuilds = rebuilds_after_a_build(&shared);
    let [first, _] = journal();
    // The batch began with a rename half pending: no cursor is safe.
    follow(&shared, &mut rebuilds, first, false);
    assert_eq!(rebuilds.save_cursor(), SaveCursor::Skip);
    assert_eq!(save(&shared, &path, rebuilds.save_cursor()), None);
    assert!(!path.exists());
    // The deferred build starts and clears the deferral, but until it publishes the index
    // still misses the change.
    let cause = Cause::new(BuildReason::UnresolvedChanges);
    rebuilds.started(&shared, cause, 11 * MINUTE);
    assert!(!rebuilds.is_deferred());
    assert_eq!(rebuilds.save_cursor(), SaveCursor::Skip);
    rebuilds.index_published();
    assert_eq!(rebuilds.save_cursor(), SaveCursor::Live);
    assert!(save(&shared, &path, rebuilds.save_cursor()).is_some());
    // A later cursor than the index's own, or another journal, never reaches the disk.
    let ahead = JournalPosition {
        next_usn: 250,
        ..START
    };
    assert_eq!(save(&shared, &path, SaveCursor::From(ahead)), None);
    let other = JournalPosition {
        journal_id: 8,
        ..START
    };
    assert_eq!(save(&shared, &path, SaveCursor::From(other)), None);
}

#[test]
fn a_lost_journal_skips_the_save_even_with_a_replay_point() {
    let shared = shared();
    let mut rebuilds = rebuilds_after_a_build(&shared);
    let [first, _] = journal();
    follow(&shared, &mut rebuilds, first, true);
    let lost = Err(NtfsError::Reenumerate("journal"));
    let followed = Followed {
        changes: lost,
        start: Some(START),
    };
    let cause = rebuilds.after_follow_from(&shared, followed, 2 * MINUTE);
    assert_eq!(cause.unwrap(), None);
    assert!(rebuilds.journal_lost());
    assert_eq!(rebuilds.save_cursor(), SaveCursor::Skip);
}
