//! Exercises crash recovery and backup quarantine with real temporary v3 snapshots.
#![allow(clippy::unwrap_used, reason = "fixture operations must succeed")]
use std::{fs, path::PathBuf};

use navo_index::{
    table::{Flags, NO_PARENT, NameTable},
    usn::{UsnBatch, UsnRecord},
};

use super::*;

const DRIVE: Drive = Drive {
    letter: 'C',
    serial: 42,
    fixed: true,
    ntfs: true,
};

/// Returns a complete searchable generation at the supplied journal cursor, in USN units.
fn index(cursor: i64) -> NtfsIndex {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let file = table.push(root, "kept.txt", Flags::NONE).unwrap();
    table.set_size(file, 9).unwrap();
    let header = file::Header {
        volume_serial: DRIVE.serial,
        journal_id: 7,
        next_usn: cursor,
        root: "C:".into(),
        built_at: 0,
    };
    NtfsIndex::restore(
        (header, table, [(5, root), (6, file)].into()),
        DRIVE.serial,
        7,
    )
    .unwrap()
}

/// Persists a full test generation through the real streamed writer and backup rotation.
fn save(path: &Path, index: &NtfsIndex) {
    let (table, ids, position) = index.snapshot().unwrap();
    let header = file::Header {
        volume_serial: DRIVE.serial,
        journal_id: position.journal_id,
        next_usn: position.next_usn,
        root: "C:".into(),
        built_at: 0,
    };
    file::save_with_ids(path, &header, table, ids).unwrap();
}

/// Applies a real BatchApply whose unresolved child still advances the cursor to 200 USN.
fn unsafe_checkpoint() -> NtfsIndex {
    let mut index = index(100);
    let mut applying = index
        .begin_batch(UsnBatch {
            next_usn: 200,
            records: vec![UsnRecord {
                file_id: 90,
                parent_id: 999,
                usn: 150,
                reason: 0x100,
                flags: Flags::NONE,
                name: "unresolved.txt".into(),
                size: Some(5),
            }],
        })
        .unwrap();
    while applying.step(&mut index, 1).unwrap() {}
    assert!(!applying.finish(&mut index).unwrap().is_empty());
    assert_eq!(index.position.next_usn, 200);
    index
}

/// Appends a marker suffix to a test snapshot path.
fn marker(path: &Path, suffix: &str) -> PathBuf {
    let mut path = path.as_os_str().to_owned();
    path.push(suffix);
    path.into()
}

/// Models a crash immediately after checkpoint replacement, before the former marker point.
fn checkpoint_then_crash(path: &Path, index: &NtfsIndex) -> Result<(), NtfsError> {
    let identity = load_source(path, DRIVE)?.0;
    quarantine::write_active(path, identity).map_err(FileError::from)?;
    save(path, index);
    Ok(())
}

/// Proves the recovered generation remains searchable while rejecting every replay and save.
fn assert_no_replay(recovery: &mut Recovery) {
    assert_eq!(
        recovery.index.table.path_with(1, '\\').as_deref(),
        Some("C:\\kept.txt")
    );
    let cursor = recovery.index.position.next_usn;
    assert!(
        recovery
            .index
            .apply_batch(UsnBatch {
                next_usn: cursor + 10,
                records: vec![]
            })
            .is_err()
    );
    assert_eq!(recovery.index.position.next_usn, cursor);
    assert!(recovery.index.snapshot().is_none());
}

#[test]
fn recovery_is_search_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("C.index");
    save(&path, &index(100));
    let checkpoint = unsafe_checkpoint();
    checkpoint_then_crash(&path, &checkpoint).unwrap();
    assert_eq!(file::load_with_ids(&path).unwrap().0.next_usn, 200);
    let mut recovered = load(&path, DRIVE).unwrap();
    let recovery_is_search_only = recovered.index.is_search_only();
    assert!(recovery_is_search_only);
    assert_eq!(recovered.source, Source::Canonical);
    assert_eq!(recovered.identity.cursor, 200);
    assert_no_replay(&mut recovered);
}

#[test]
fn backup_recovery_is_search_only() {
    for keep_complete in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("C.index");
        let checkpoint = unsafe_checkpoint();
        save(&path, &checkpoint);
        let unsafe_id = load_source(&path, DRIVE).unwrap().0;
        quarantine::write_active(&path, unsafe_id).unwrap();
        save(&path, &index(300));
        let complete_id = load_source(&path, DRIVE).unwrap().0;
        quarantine::write_complete(&path, complete_id).unwrap();
        if !keep_complete {
            quarantine::clear(&path, complete_id).unwrap();
        }
        let backup = navo_store::backup_path(&path);
        let backup_bytes = fs::read(&backup).unwrap();
        assert_eq!(file::load_with_ids(&backup).unwrap().0.next_usn, 200);
        fs::write(&path, b"corrupt canonical").unwrap();
        let corrupt = fs::read(&path).unwrap();
        let mut recovered = load(&path, DRIVE).unwrap();
        let backup_recovery_is_search_only = recovered.index.is_search_only();
        assert!(backup_recovery_is_search_only);
        assert_eq!(recovered.source, Source::Backup);
        assert_eq!(recovered.identity, unsafe_id);
        assert_eq!(
            quarantine::read(&path, unsafe_id).unwrap(),
            Marker::SearchOnly
        );
        assert!(
            fs::read_to_string(marker(&path, ".rebuilding"))
                .unwrap()
                .contains(":active:")
        );
        assert_no_replay(&mut recovered);
        assert_eq!(fs::read(&backup).unwrap(), backup_bytes);
        assert_eq!(fs::read(&path).unwrap(), corrupt);
    }
}

#[test]
fn quarantine_canonical_marker_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("C.index");
    save(&path, &index(100));
    let id = load(&path, DRIVE).unwrap().identity;
    assert!(!load(&path, DRIVE).unwrap().index.is_search_only());
    quarantine::write_complete(&path, id).unwrap();
    assert!(!load(&path, DRIVE).unwrap().index.is_search_only());
    quarantine::write_active(&path, id).unwrap();
    assert!(
        load(&path, DRIVE).unwrap().index.is_search_only(),
        "active canonical cannot resume"
    );
    for bytes in [
        "",
        "unknown",
        "NAVO-QUARANTINE-1:complete:42",
        &"x".repeat(10_000),
    ] {
        fs::write(marker(&path, ".rebuilding"), bytes).unwrap();
        let mut recovered = load(&path, DRIVE).unwrap();
        assert!(recovered.index.is_search_only());
        assert_no_replay(&mut recovered);
    }
    for wrong in [
        Identity { serial: 43, ..id },
        Identity { journal: 8, ..id },
        Identity { cursor: 101, ..id },
        Identity {
            checksum: id.checksum ^ 1,
            ..id
        },
    ] {
        quarantine::write_complete(&path, wrong).unwrap();
        assert!(load(&path, DRIVE).unwrap().index.is_search_only());
    }
    quarantine::write_complete(&path, id).unwrap();
    quarantine::clear(&path, id).unwrap();
    assert!(!load(&path, DRIVE).unwrap().index.is_search_only());
    for suffix in [".rebuilding.tmp", ".rebuilding.pending"] {
        fs::write(marker(&path, suffix), b"unfinished").unwrap();
        assert!(load(&path, DRIVE).unwrap().index.is_search_only());
        fs::remove_file(marker(&path, suffix)).unwrap();
    }
}

#[test]
fn quarantine_rejects_wrong_identity_or_checksum() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("C.index");
    save(&path, &index(100));
    assert!(
        load(
            &path,
            Drive {
                serial: 43,
                ..DRIVE
            }
        )
        .is_err(),
        "wrong serial must not be published"
    );
    assert!(
        load(
            &path,
            Drive {
                letter: 'D',
                ..DRIVE
            }
        )
        .is_err()
    );
    let mut bytes = fs::read(&path).unwrap();
    bytes[66] ^= 1;
    fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        file::load_with_ids(&path),
        Err(FileError::BadChecksum)
    ));
    assert!(load(&path, DRIVE).is_err());
    save(&navo_store::backup_path(&path), &index(100));
    let recovered = load(&path, DRIVE).unwrap();
    assert_eq!(recovered.source, Source::Backup);
    assert!(recovered.index.is_search_only());
    fs::remove_file(&path).unwrap();
    assert_eq!(load(&path, DRIVE).unwrap().source, Source::Backup);
}

#[test]
fn quarantine_backup_marker_failure_stops_before_repair() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("C.index");
    save(&navo_store::backup_path(&path), &unsafe_checkpoint());
    fs::write(&path, b"corrupt canonical").unwrap();
    let backup = fs::read(navo_store::backup_path(&path)).unwrap();
    fs::create_dir(marker(&path, ".rebuilding.tmp")).unwrap();
    let mut reached_repair = false;
    let result = load(&path, DRIVE).map(|_| reached_repair = true);
    assert!(
        result.is_err(),
        "marker refusal must stop before returning a repair source"
    );
    assert!(!reached_repair);
    assert_eq!(fs::read(&path).unwrap(), b"corrupt canonical");
    assert_eq!(fs::read(navo_store::backup_path(&path)).unwrap(), backup);
}

#[test]
fn quarantine_marker_failure_stops_checkpoint_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("C.index");
    save(&path, &index(100));
    let original = fs::read(&path).unwrap();
    fs::create_dir(marker(&path, ".rebuilding.tmp")).unwrap();
    let mut checkpoint = unsafe_checkpoint();
    let mut reached_conversion = false;
    let result = checkpoint_then_crash(&path, &checkpoint).map(|()| {
        reached_conversion = true;
        checkpoint.make_search_only();
    });
    assert!(
        result.is_err(),
        "marker refusal must stop checkpoint replacement"
    );
    assert!(!reached_conversion);
    assert!(!checkpoint.is_search_only());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(!navo_store::backup_path(&path).exists());
}
