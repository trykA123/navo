//! Proves a failed, cancelled or successful build treats the snapshot on disk as IXH1 requires.

use std::path::Path;

use navo_index::file::{self, Header};
use navo_index::ntfs::NtfsError;
use navo_index::table::{Flags, NO_PARENT, NameTable};

use super::super::config::Drive;
use super::super::lifecycle::restore_or_enumerate;
use super::{discard_snapshot, enumerate_then_discard};

const DRIVE: Drive = Drive {
    letter: 'C',
    serial: 42,
    fixed: true,
    ntfs: true,
};

/// Writes a one-folder snapshot for drive C: at journal 7, cursor 100.
fn write_snapshot(path: &Path) {
    let mut table = NameTable::new();
    table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let header = Header {
        volume_serial: 42,
        journal_id: 7,
        next_usn: 100,
        root: "C:".into(),
        built_at: 0,
    };
    file::save_with_ids(path, &header, &table, &[(5, 0)].into()).unwrap();
}

/// Asserts the next start restores the snapshot at `path` instead of building again.
fn assert_next_start_restores(path: &Path) {
    let restored = restore_or_enumerate(file::load_with_ids(path), DRIVE, |reason| {
        panic!("the next start must restore the snapshot, not build ({reason:?})")
    })
    .unwrap();
    assert_eq!(restored.position.next_usn, 100);
}

#[test]
fn a_failed_enumeration_keeps_the_snapshot_for_the_next_start() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("volume.index");
    write_snapshot(&path);
    let failed = enumerate_then_discard(
        || -> Result<(), NtfsError> { Err(NtfsError::InvalidResponse("enumeration failed")) },
        || discard_snapshot(&path),
    );
    assert!(matches!(failed, Err(NtfsError::InvalidResponse(_))));
    assert!(
        path.exists(),
        "a failed build must not remove the valid snapshot"
    );
    assert_next_start_restores(&path);
}

#[test]
fn a_cancelled_enumeration_keeps_the_snapshot_for_the_next_start() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("volume.index");
    write_snapshot(&path);
    let cancelled = enumerate_then_discard(
        || -> Result<(), NtfsError> { Err(NtfsError::Cancelled) },
        || discard_snapshot(&path),
    );
    assert!(matches!(cancelled, Err(NtfsError::Cancelled)));
    assert_next_start_restores(&path);
}

#[test]
fn a_finished_enumeration_discards_the_old_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("volume.index");
    write_snapshot(&path);
    let (candidate, discarded) =
        enumerate_then_discard(|| Ok::<_, NtfsError>(7), || discard_snapshot(&path)).unwrap();
    assert_eq!((candidate, discarded), (7, true));
    assert!(!path.exists());
}

#[test]
fn a_failed_discard_stops_the_build() {
    let directory = tempfile::tempdir().unwrap();
    // A directory at the snapshot path makes remove_file fail with an error other than NotFound.
    let path = directory.path().join("volume.index");
    std::fs::create_dir(&path).unwrap();
    let result = enumerate_then_discard(|| Ok::<_, NtfsError>(7), || discard_snapshot(&path));
    assert!(matches!(result, Err(NtfsError::Io { .. })));
    assert!(path.exists());
}
