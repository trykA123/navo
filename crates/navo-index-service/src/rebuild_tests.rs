use std::sync::{Arc, RwLock};
use std::time::Duration;

use navo_index::file::Header;
use navo_index::ntfs::{NtfsError, NtfsIndex};
use navo_index::pipe::{BuildReason, CancelToken, QueryTerm, Request, VolumeBuilds};
use navo_index::table::{Flags, NO_PARENT, NameTable};
use navo_index::usn::{FrnMap, UsnBatch, UsnRecord};

use super::super::answer::{Shared, State};
use super::super::batch::{apply_batch, published};
use super::super::config::Drive;
use super::super::lifecycle::restore_or_enumerate;
use super::super::search::SearchIndex;
use super::super::service_log::{LOG_NAME, Log};
use super::*;

const MINUTE: Duration = Duration::from_secs(60);

#[path = "rebuild_reset_tests.rs"]
mod reset;

/// Fakes MFT enumeration failing before it can return a candidate index.
fn enumerate_io_error() -> Result<NtfsIndex, NtfsError> {
    Err(NtfsError::Io {
        operation: "enumerate MFT",
        source: std::io::Error::from_raw_os_error(5),
    })
}

/// Keeps the published generation and resumes replay if verification cannot enumerate.
#[test]
fn a_failed_verification_keeps_the_published_index_and_reschedules() {
    let shared = shared();
    let (directory, log) = logged();
    let mut rebuilds = Rebuilds::new('C', log);
    let original = published(&shared).unwrap();
    let now = Instant::now();
    let due = now + 10 * MINUTE;
    rebuilds.schedule_verification(now, "build_cycle");
    let reason = rebuilds
        .verification_due(due)
        .expect("first verification is due");
    let verified = rebuilds.verification_result(enumerate_io_error(), reason, due);
    assert!(
        verified
            .expect("verification failure must allow the follow tick")
            .is_none()
    );
    let live = published(&shared).unwrap();
    assert!(Arc::ptr_eq(&original, &live));
    assert_eq!(live.position.next_usn, 100);
    let search = SearchIndex::new(&navo_store::Search {
        drives: vec!["C:\\".into()],
        ..Default::default()
    });
    let request = Request {
        terms: vec![QueryTerm::Name("kept".into())],
    };
    let hits = search.hits(&live.table, &request, &CancelToken::new());
    assert_eq!(hits[0].path, "C:\\kept.txt");
    assert_eq!(rebuilds.take_retry_delay(), None);
    drop((original, live));

    let changes = follow(&shared, 120, vec![record(7, 5, 0x100)]);
    assert!(changes.is_empty());
    assert_eq!(entries_and_position(&shared), (3, 120));
    let before_retry = due + 19 * MINUTE;
    assert_eq!(rebuilds.verification_due(before_retry), None);
    assert!(rebuilds.verification_due(due + 20 * MINUTE).is_some());
    assert!(log_text(&directory).contains("verify scheduled C: in 20 min (build_cycle)"));
}

/// Applies the full-build retry delays to consecutive cold-start enumeration errors.
#[test]
fn a_cold_build_io_error_backs_off() {
    let mut rebuilds = Rebuilds::new('C', Log::default());
    let drive = Drive {
        letter: 'C',
        serial: 42,
        fixed: true,
        ntfs: true,
    };
    for delay in [2, 4] {
        let missing = Err(FileError::Io(std::io::ErrorKind::NotFound.into()));
        let result = restore_or_enumerate(missing, drive, |reason| {
            assert_eq!(reason, BuildReason::SnapshotMissing);
            rebuilds.build_result(enumerate_io_error())
        });
        assert!(matches!(result, Err(NtfsError::Io { .. })));
        assert_eq!(
            rebuilds.take_retry_delay(),
            Some(Duration::from_secs(delay))
        );
    }
}

/// Publishes an inexact result, runs one verification, and keeps searching after it succeeds.
#[test]
fn an_inexact_build_schedules_one_verification_and_an_exact_result_stops_it() {
    let shared = shared();
    let (directory, log) = logged();
    let mut rebuilds = Rebuilds::new('C', log);
    let now = Instant::now();
    let initial = Cause::new(BuildReason::SnapshotMissing);
    rebuilds.started(&shared, initial, Duration::ZERO);
    rebuilds.build_quality(Some(InexactBuild::Cycle), now);
    rebuilds.build_quality(Some(InexactBuild::Cycle), now + MINUTE);
    let early = 10 * MINUTE;
    assert_eq!(rebuilds.verification_due(now + early), None);
    assert_eq!(entries_and_position(&shared), (2, 100));
    let verification = rebuilds
        .verification_due(now + 11 * MINUTE)
        .expect("verification is due");
    assert_eq!(rebuilds.verification_due(now + 11 * MINUTE), None);
    assert_eq!(verification, "build_cycle");
    rebuilds.started(&shared, Cause::verification(), 10 * MINUTE);
    rebuilds.build_quality(None, now + 10 * MINUTE);
    assert_eq!(rebuilds.verification_due(now + 90 * MINUTE), None);
    assert_eq!(builds(&shared).full_builds, 2);
    let text = log_text(&directory);
    assert_eq!(
        text.matches("verify scheduled C: in 10 min (build_cycle)")
            .count(),
        1
    );
}

/// Checks every delay through the cap and one failure beyond it.
#[test]
fn build_backoff_doubles_then_caps_the_delay_at_300_seconds() {
    let mut backoff = BuildBackoff::default();
    for seconds in [2, 4, 8, 16, 32, 64, 128, 256, 300, 300] {
        assert_eq!(backoff.failed(), Duration::from_secs(seconds));
    }
}

/// Starts a new failure streak at the first delay after a success.
#[test]
fn build_backoff_resets_after_a_successful_build() {
    let mut backoff = BuildBackoff::default();
    backoff.failed();
    backoff.failed();
    backoff.succeeded();
    assert_eq!(backoff.0, 0);
    assert_eq!(backoff.failed(), Duration::from_secs(2));
}

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
    let wrapped =
        rebuilds.after_follow(&shared, Err(NtfsError::Reenumerate("journal")), 6 * MINUTE);
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
    assert_eq!(due.reason, BuildReason::JournalReset);
    rebuilds.started(&shared, due, 11 * MINUTE);
    rebuilds.finished(&shared, 2, Duration::from_millis(1500));
    assert!(
        rebuilds.journal_lost(),
        "a reset stays pending until publication"
    );
    rebuilds.regular_build_published();
    assert!(!rebuilds.is_deferred());
    assert_eq!(rebuilds.due(30 * MINUTE), None);
    assert_eq!(builds(&shared).full_builds, 2);

    let text = log_text(&directory);
    assert!(text.contains(
        "full build start C: reason=unresolved_changes blocked_add=1 blocked_rename=0 attributes=0 size=1 other=0"
    ));
    assert!(text.contains("full build done C: entries=2 duration_ms=1500"));
    assert!(text.contains("update error C: reenumerate(journal)"));
    assert!(text.contains("rebuild deferred C: reason=unresolved_changes"));
    assert_eq!(text.matches("rebuild deferred").count(), 1);
    assert!(!text.contains("secret-name"));
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

/// AF9: a skipped folder added back names itself as missing; it counts as `other`, not as an
/// add blocked by its parent.
#[test]
fn a_skipped_folder_added_back_counts_as_other() {
    let change = navo_index::table::TableChange::Add {
        file_id: 30,
        parent_id: 5,
        name: "docs".into(),
        flags: Flags::FOLDER,
    };
    let rejoined = navo_index::ntfs::UnresolvedChange {
        missing_reference: 30,
        change,
    };
    let text = Unresolved::count(&[rejoined]).to_string();
    assert!(
        text.starts_with("blocked_add=0 blocked_rename=0 attributes=0 size=0 other=1"),
        "{text}"
    );
}

/// The rebuild reason names the first unresolved references, so a VM log identifies them.
#[test]
fn the_rebuild_reason_names_the_first_three_unresolved_references() {
    let shared = shared();
    let mut rebuilds = Rebuilds::new('C', Log::default());
    let created = |n: u128| UsnRecord {
        size: None,
        ..record(901 + n, 777, 0x100)
    };
    let records = (0..4).map(created).collect();
    let changes = follow(&shared, 120, records);
    let cause = rebuilds
        .after_follow(&shared, Ok(changes), Duration::ZERO)
        .unwrap()
        .expect("nothing was built yet");
    let text = cause.to_string();
    assert!(
        text.ends_with(" first_missing:file=777:901,777:902,777:903"),
        "{text}"
    );
}
