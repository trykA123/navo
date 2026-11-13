//! Exercises rollback decisions through production startup and real snapshot bytes.
#![allow(clippy::unwrap_used, reason = "fixture operations must succeed")]
use std::{fs, path::PathBuf};

use navo_index::{
    ntfs::test_support::MftFixture,
    pipe::BuildReason,
    usn::{UsnBatch, UsnRecord},
};

use super::*;

const DRIVE: Drive = Drive {
    letter: 'C',
    serial: 42,
    fixed: true,
    ntfs: true,
};

struct Fixture {
    directory: tempfile::TempDir,
    canonical: PathBuf,
    selected: PathBuf,
    bytes: Vec<u8>,
    marker_bytes: Vec<u8>,
}

/// Builds an advanced-cursor snapshot and active marker; never performs rollback or preservation.
fn fixture(source: Source) -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let canonical = directory.path().join("C.index");
    let selected = match source {
        Source::Canonical => canonical.clone(),
        Source::Backup => navo_store::backup_path(&canonical),
    };
    let mut index = MftFixture::new(2, 7).read().index;
    let unresolved = index
        .apply_batch(UsnBatch {
            next_usn: 200,
            records: vec![UsnRecord {
                file_id: 90,
                parent_id: 999,
                usn: 150,
                reason: 0x100,
                flags: navo_index::table::Flags::NONE,
                name: "unresolved.txt".into(),
                size: Some(5),
            }],
        })
        .unwrap();
    assert!(!unresolved.is_empty());
    assert_eq!(index.position.next_usn, 200);
    assert!(!index.ids.contains_key(&90));
    let (table, ids, position) = index.snapshot().unwrap();
    let header = file::Header {
        volume_serial: DRIVE.serial,
        journal_id: position.journal_id,
        next_usn: position.next_usn,
        root: "C:".into(),
        built_at: 0,
    };
    file::save_with_ids(&selected, &header, table, ids).unwrap();
    let identity = load_source(&selected, DRIVE).unwrap().0;
    quarantine::write_active(&canonical, identity).unwrap();
    let bytes = fs::read(&selected).unwrap();
    let marker_bytes = fs::read(directory.path().join("C.index.rebuilding")).unwrap();
    Fixture {
        directory,
        canonical,
        selected,
        bytes,
        marker_bytes,
    }
}

#[test]
fn quarantine_rollback_cold_enumeration() {
    for source in [Source::Canonical, Source::Backup] {
        let fixture = fixture(source);
        let preserved = fixture.directory.path().join("rollback-2026-11-13");
        let mut cursors = Vec::new();
        let rebuilt = rollback(
            Rollback {
                path: &fixture.canonical,
                preserved: &preserved,
                drive: DRIVE,
            },
            |reason| {
                assert_eq!(reason, BuildReason::SnapshotMissing);
                let (scan, requests) = MftFixture::new(3, 7).read_with_cursors();
                cursors = requests;
                Ok(scan.index)
            },
        );
        assert_eq!(
            cursors.first(),
            Some(&0),
            "rollback must cold-enumerate instead of resuming an incomplete checkpoint: {source:?}"
        );
        assert_eq!(cursors, [0, 3]);
        let rebuilt = rebuilt.unwrap();
        assert_eq!(rebuilt.position.next_usn, 100);
        assert_eq!(rebuilt.table.len(), 3);
        assert!(!fixture.canonical.exists());
        assert!(!navo_store::backup_path(&fixture.canonical).exists());
        assert_eq!(
            fs::read(preserved.join(fixture.selected.file_name().unwrap())).unwrap(),
            fixture.bytes
        );
        assert_eq!(
            fs::read(preserved.join("C.index.rebuilding")).unwrap(),
            fixture.marker_bytes
        );
        assert!(!fixture.directory.path().join("C.index.rebuilding").exists());
    }
}

#[test]
fn quarantine_rollback_preserves_sole_backup() {
    let fixture = fixture(Source::Backup);
    assert!(!fixture.canonical.exists());
    let preserved = fixture.directory.path().join("rollback-2026-11-13");
    let result = rollback(
        Rollback {
            path: &fixture.canonical,
            preserved: &preserved,
            drive: DRIVE,
        },
        |_| Err(NtfsError::Cancelled),
    );
    assert_eq!(
        fs::read(preserved.join("C.index.bak")).ok(),
        Some(fixture.bytes),
        "rollback must preserve sole backup bytes even when enumeration fails"
    );
    assert!(matches!(result, Err(NtfsError::Cancelled)));
    assert!(!fixture.canonical.exists());
    assert!(!fixture.selected.exists());
    assert_eq!(
        fs::read(preserved.join("C.index.rebuilding")).unwrap(),
        fixture.marker_bytes
    );
}
