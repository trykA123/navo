use std::sync::{Arc, RwLock};
use std::time::Duration;

use navo_index::file::Header;
use navo_index::ntfs::{NtfsError, NtfsIndex};
use navo_index::pipe::{BuildReason, VolumeBuilds};
use navo_index::table::{Flags, NO_PARENT, NameTable};
use navo_index::usn::{FrnMap, UsnBatch, UsnRecord};

use super::super::answer::{Shared, State};
use super::super::batch::apply_batch;
use super::super::service_log::{LOG_NAME, Log};
use super::*;

const MINUTE: Duration = Duration::from_secs(60);

fn shared() -> Shared {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let file = table.push(root, "kept.txt", Flags::NONE).unwrap();
    let ids: FrnMap = [(5, root), (6, file)].into();
    let header = Header {
        volume_serial: 42,
        journal_id: 7,
        next_usn: 100,
        root: "C:".into(),
        built_at: 0,
    };
    let index = NtfsIndex::restore((header, table, ids), 42, 7).unwrap();
    Arc::new(RwLock::new(State {
        index: Some(Arc::new(index)),
        ..State::default()
    }))
}

fn record(file_id: u128, parent_id: u128, reason: u32) -> UsnRecord {
    UsnRecord {
        file_id,
        parent_id,
        usn: 100,
        reason,
        flags: Flags::NONE,
        name: "secret-name.txt".into(),
        size: Some(7),
    }
}

/// Applies one journal batch to the shared index, as the worker's follow does.
fn follow(shared: &Shared, next_usn: i64, records: Vec<UsnRecord>) -> Vec<UnresolvedChange> {
    apply_batch(shared, UsnBatch { next_usn, records }).unwrap()
}

fn overwrite_unknown_file(shared: &Shared, next_usn: i64) -> Vec<UnresolvedChange> {
    follow(shared, next_usn, vec![record(900, 5, 0x1)])
}

fn create_under_unknown_folder(shared: &Shared, next_usn: i64) -> Vec<UnresolvedChange> {
    follow(shared, next_usn, vec![record(901, 777, 0x100)])
}

fn builds(shared: &Shared) -> VolumeBuilds {
    shared.read().unwrap().builds.clone()
}

fn entries_and_position(shared: &Shared) -> (usize, i64) {
    let state = shared.read().unwrap();
    let index = state.index.as_ref().expect("the index keeps answering");
    (index.table.len(), index.position.next_usn)
}

fn logged() -> (tempfile::TempDir, Log) {
    let directory = tempfile::tempdir().unwrap();
    let log = Log::open(directory.path());
    (directory, log)
}

fn log_text(directory: &tempfile::TempDir) -> String {
    std::fs::read_to_string(directory.path().join(LOG_NAME)).unwrap_or_default()
}

#[test]
fn attribute_or_size_changes_for_an_unknown_file_leave_the_index_and_start_no_build() {
    let shared = shared();
    let (directory, log) = logged();
    let mut rebuilds = Rebuilds::new('C', log);
    let changes = overwrite_unknown_file(&shared, 120);
    assert_eq!(
        changes.len(),
        2,
        "the overwrite reports attributes and size"
    );
    let unresolved = Unresolved::count(&changes);
    assert!(!unresolved.needs_rebuild());
    let decision = rebuilds.after_follow(&shared, Ok(changes), Duration::ZERO);
    assert_eq!(decision.unwrap(), None);
    assert_eq!(entries_and_position(&shared), (2, 120));
    assert!(!rebuilds.is_deferred());
    let counters = builds(&shared);
    assert_eq!((counters.full_builds, counters.unresolved_batches), (0, 1));
    assert_eq!(counters.dropped_changes, 2);
    assert!(!log_text(&directory).contains("full build"));
}

#[test]
fn a_blocked_create_without_an_earlier_build_starts_one_at_once() {
    let shared = shared();
    let mut rebuilds = Rebuilds::new('C', Log::default());
    let changes = create_under_unknown_folder(&shared, 120);
    let cause = rebuilds
        .after_follow(&shared, Ok(changes), Duration::ZERO)
        .unwrap()
        .expect("nothing was built yet");
    assert_eq!(cause.reason, BuildReason::UnresolvedChanges);
    assert!(
        cause
            .unresolved
            .is_some_and(|counts| counts.needs_rebuild())
    );
}

#[test]
fn two_rebuild_causes_within_the_window_give_one_build_and_the_deferred_one_runs_after_it() {
    let shared = shared();
    let (directory, log) = logged();
    let mut rebuilds = Rebuilds::new('C', log);
    let first = create_under_unknown_folder(&shared, 120);
    let cause = rebuilds
        .after_follow(&shared, Ok(first), MINUTE)
        .unwrap()
        .unwrap();
    rebuilds.started(&shared, cause, MINUTE);
    rebuilds.finished(&shared, 2, Duration::from_millis(1500));

    let second = create_under_unknown_folder(&shared, 140);
    let later = rebuilds.after_follow(&shared, Ok(second), 5 * MINUTE);
    assert_eq!(later.unwrap(), None, "the second cause must wait");
    let wrapped = rebuilds.after_follow(&shared, Err(NtfsError::Reenumerate), 6 * MINUTE);
    assert_eq!(wrapped.unwrap(), None);
    assert!(rebuilds.is_deferred());
    assert_eq!(entries_and_position(&shared), (2, 140));
    let counters = builds(&shared);
    assert_eq!(
        (
            counters.full_builds,
            counters.deferred_rebuilds,
            counters.unresolved_batches
        ),
        (1, 1, 2)
    );
    assert_eq!(counters.last_duration_ms, 1500);

    assert_eq!(rebuilds.due(10 * MINUTE + MINUTE / 2), None);
    let due = rebuilds.due(11 * MINUTE).expect("the window has passed");
    assert_eq!(due.reason, BuildReason::UnresolvedChanges);
    rebuilds.started(&shared, due, 11 * MINUTE);
    assert!(!rebuilds.is_deferred());
    assert_eq!(rebuilds.due(30 * MINUTE), None);
    assert_eq!(builds(&shared).full_builds, 2);

    let text = log_text(&directory);
    assert!(text.contains(
        "full build start C: reason=unresolved_changes blocked_add=1 blocked_rename=0 attributes=0 size=1 other=0"
    ));
    assert!(text.contains("full build done C: entries=2 duration_ms=1500"));
    assert!(text.contains("rebuild deferred C: reason=unresolved_changes"));
    assert_eq!(text.matches("rebuild deferred").count(), 1);
    assert!(!text.contains("secret-name"));
}

#[test]
fn a_journal_reset_after_the_window_builds_at_once() {
    let shared = shared();
    let mut rebuilds = Rebuilds::new('C', Log::default());
    rebuilds.started(
        &shared,
        Cause::new(BuildReason::SnapshotMissing),
        Duration::ZERO,
    );
    let decision = rebuilds.after_follow(&shared, Err(NtfsError::Reenumerate), REBUILD_WINDOW);
    assert_eq!(
        decision.unwrap(),
        Some(Cause::new(BuildReason::JournalReset))
    );
    assert_eq!(
        builds(&shared).last_reason,
        Some(BuildReason::SnapshotMissing)
    );
}

#[test]
fn other_follow_errors_pass_through_without_counting() {
    let shared = shared();
    let mut rebuilds = Rebuilds::new('C', Log::default());
    let failed = rebuilds.after_follow(&shared, Err(NtfsError::FileGone), Duration::ZERO);
    assert!(matches!(failed, Err(NtfsError::FileGone)));
    assert_eq!(builds(&shared), VolumeBuilds::default());
}

#[test]
fn error_kinds_carry_codes_but_no_text_from_the_system() {
    let error = NtfsError::Io {
        operation: "read journal",
        source: std::io::Error::from_raw_os_error(5),
    };
    assert_eq!(error_kind(&error), "io op=\"read journal\" os_error=5");
    let snapshot = NtfsError::Snapshot(navo_index::file::FileError::BadChecksum);
    assert_eq!(error_kind(&snapshot), "snapshot_bad_checksum");
}
