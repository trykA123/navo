//! Tests the once-only USN journal create on ERROR_JOURNAL_NOT_ACTIVE (IXJ).

use std::collections::VecDeque;

use navo_index::ntfs::NtfsError;

use super::journal_create::{
    ALLOCATION_DELTA, JournalCheck, JournalControl, JournalCreate, MAXIMUM_SIZE,
};
use super::service_log::{LOG_NAME, Log};

/// Scripted volume: answers queries and creates from queues and records each create's sizes.
#[derive(Default)]
struct FakeVolume {
    queries: VecDeque<Option<i32>>,
    creates: VecDeque<Option<i32>>,
    created: Vec<(u64, u64)>,
}

/// Builds an OS error as the Windows volume reports it.
fn os_error(code: i32) -> NtfsError {
    NtfsError::Io {
        operation: "volume control",
        source: std::io::Error::from_raw_os_error(code),
    }
}

impl JournalControl for FakeVolume {
    fn query_journal(&mut self) -> Result<(), NtfsError> {
        match self.queries.pop_front().flatten() {
            None => Ok(()),
            Some(code) => Err(os_error(code)),
        }
    }

    fn create_journal(
        &mut self,
        maximum_size: u64,
        allocation_delta: u64,
    ) -> Result<(), NtfsError> {
        self.created.push((maximum_size, allocation_delta));
        match self.creates.pop_front().flatten() {
            None => Ok(()),
            Some(code) => Err(os_error(code)),
        }
    }
}

/// Opens a service log in a fresh folder; returns the folder guard and the log.
fn log() -> (tempfile::TempDir, Log) {
    let folder = tempfile::tempdir().unwrap();
    let log = Log::open(folder.path());
    (folder, log)
}

/// Reads the service log text written so far.
fn read(folder: &tempfile::TempDir) -> String {
    std::fs::read_to_string(folder.path().join(LOG_NAME)).unwrap_or_default()
}

#[test]
fn a_missing_journal_is_created_with_fsutil_defaults_and_logged_once() {
    let (folder, log) = log();
    let mut volume = FakeVolume {
        queries: VecDeque::from([Some(1179)]),
        ..FakeVolume::default()
    };
    let check = JournalCreate::default().ensure(&mut volume, 'S', &log);
    assert_eq!(check, JournalCheck::Created);
    assert_eq!(volume.created, vec![(33_554_432, 8_388_608)]);
    assert_eq!((MAXIMUM_SIZE, ALLOCATION_DELTA), (33_554_432, 8_388_608));
    let text = read(&folder);
    assert_eq!(text.matches("usn journal created S:").count(), 1, "{text}");
}

#[test]
fn an_active_journal_or_another_error_creates_nothing() {
    let (folder, log) = log();
    let mut create = JournalCreate::default();
    for query in [None, Some(5), Some(1178), Some(1181)] {
        let mut volume = FakeVolume {
            queries: VecDeque::from([query]),
            ..FakeVolume::default()
        };
        assert_eq!(
            create.ensure(&mut volume, 'S', &log),
            JournalCheck::Untouched
        );
        assert!(
            volume.created.is_empty(),
            "query {query:?} created a journal"
        );
    }
    assert_eq!(read(&folder), "");
    // A later missing journal still gets its one create: the passes above spent no attempt.
    let mut volume = FakeVolume {
        queries: VecDeque::from([Some(1179)]),
        ..FakeVolume::default()
    };
    assert_eq!(create.ensure(&mut volume, 'S', &log), JournalCheck::Created);
}

#[test]
fn a_failed_create_warns_once_and_is_never_retried() {
    let (folder, log) = log();
    let mut create = JournalCreate::default();
    let mut volume = FakeVolume {
        queries: VecDeque::from([Some(1179), Some(1179), Some(1179)]),
        creates: VecDeque::from([Some(5)]),
        ..FakeVolume::default()
    };
    assert_eq!(
        create.ensure(&mut volume, 'S', &log),
        JournalCheck::CreateFailed
    );
    assert_eq!(
        create.ensure(&mut volume, 'S', &log),
        JournalCheck::AlreadyTried
    );
    assert_eq!(
        create.ensure(&mut volume, 'S', &log),
        JournalCheck::AlreadyTried
    );
    assert_eq!(volume.created.len(), 1, "the create ran more than once");
    let text = read(&folder);
    assert_eq!(
        text.matches("usn journal create failed S:").count(),
        1,
        "{text}"
    );
    assert!(text.contains("os_error=5"), "{text}");
    assert!(!text.contains("usn journal created"), "{text}");
}

#[test]
fn a_journal_missing_again_after_a_create_is_not_created_twice() {
    let (_folder, log) = log();
    let mut create = JournalCreate::default();
    let mut volume = FakeVolume {
        queries: VecDeque::from([Some(1179), Some(1179)]),
        ..FakeVolume::default()
    };
    assert_eq!(create.ensure(&mut volume, 'S', &log), JournalCheck::Created);
    assert_eq!(
        create.ensure(&mut volume, 'S', &log),
        JournalCheck::AlreadyTried
    );
    assert_eq!(volume.created.len(), 1);
}
