use std::fs;
use std::path::{Path, PathBuf};

use super::*;

fn suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    name.into()
}

fn database(dir: &tempfile::TempDir) -> PathBuf {
    let path = dir.path().join("ferrite.db");
    fs::write(&path, b"damaged database").unwrap();
    path
}

#[test]
fn database_and_sidecars_keep_their_exact_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    fs::write(suffix(&path, "-wal"), b"last committed pages").unwrap();
    fs::write(suffix(&path, "-shm"), b"shared memory").unwrap();
    let report = quarantine_database(&path).unwrap().unwrap();
    assert_eq!(report.slot, 1);
    assert_eq!(
        fs::read(report.database.unwrap()).unwrap(),
        b"damaged database"
    );
    assert_eq!(
        fs::read(report.wal.unwrap()).unwrap(),
        b"last committed pages"
    );
    assert_eq!(fs::read(report.shm.unwrap()).unwrap(), b"shared memory");
    assert!(!path.exists());
    assert!(!suffix(&path, "-wal").exists());
    assert!(!suffix(&path, "-shm").exists());
    assert!(!suffix(&path, ".quarantine-pending").exists());
}

#[test]
fn an_existing_sidecar_reserves_its_whole_slot() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    let occupied = suffix(&path, ".corrupt-1-shm");
    fs::write(&occupied, b"keep old evidence").unwrap();
    let report = quarantine_database(&path).unwrap().unwrap();
    assert_eq!(report.slot, 2);
    assert_eq!(fs::read(occupied).unwrap(), b"keep old evidence");
}

#[test]
fn a_pending_record_finishes_only_its_recorded_slot() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    fs::rename(&path, suffix(&path, ".corrupt-2")).unwrap();
    fs::write(suffix(&path, ".corrupt-9"), b"unrelated database").unwrap();
    fs::write(suffix(&path, "-wal"), b"remaining wal").unwrap();
    fs::write(suffix(&path, ".quarantine-pending"), b"2 database\n").unwrap();
    let report = quarantine_database(&path).unwrap().unwrap();
    assert_eq!(report.slot, 2);
    assert_eq!(fs::read(report.wal.unwrap()).unwrap(), b"remaining wal");
    assert!(!suffix(&path, ".corrupt-9-wal").exists());
}

#[test]
fn orphan_sidecars_never_attach_to_an_unrelated_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    fs::write(suffix(&path, ".corrupt-1"), b"older database").unwrap();
    fs::write(suffix(&path, "-wal"), b"unidentified wal").unwrap();
    let report = quarantine_database(&path).unwrap().unwrap();
    assert_eq!(report.slot, 2);
    assert!(report.database.is_none());
    assert_eq!(fs::read(report.wal.unwrap()).unwrap(), b"unidentified wal");
    assert!(!suffix(&path, ".corrupt-1-wal").exists());
    assert!(!path.exists());
}

#[test]
fn a_collision_during_resume_preserves_both_copies() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    let backup = suffix(&path, ".corrupt-1");
    fs::write(&backup, b"existing evidence").unwrap();
    fs::write(suffix(&path, ".quarantine-pending"), b"1 database\n").unwrap();
    assert!(matches!(
        quarantine_database(&path),
        Err(RecoveryError::AmbiguousMove { .. })
    ));
    assert_eq!(fs::read(path).unwrap(), b"damaged database");
    assert_eq!(fs::read(backup).unwrap(), b"existing evidence");
}

#[test]
fn malformed_pending_records_leave_every_file_alone() {
    for record in ["", "0 database", "../outside database", "1 database extra"] {
        let dir = tempfile::tempdir().unwrap();
        let path = database(&dir);
        fs::write(suffix(&path, ".quarantine-pending"), record).unwrap();
        assert!(matches!(
            quarantine_database(&path),
            Err(RecoveryError::InvalidRecord(_))
        ));
        assert_eq!(fs::read(path).unwrap(), b"damaged database");
    }
}

#[test]
fn another_recovery_owner_prevents_any_move() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(suffix(&path, ".quarantine-lock"))
        .unwrap();
    lock.lock().unwrap();
    assert!(matches!(
        quarantine_database(&path),
        Err(RecoveryError::Busy(_))
    ));
    assert_eq!(fs::read(path).unwrap(), b"damaged database");
}

#[cfg(unix)]
#[test]
fn symbolic_links_are_never_quarantined_or_followed() {
    let dir = tempfile::tempdir().unwrap();
    let outside = dir.path().join("outside.db");
    fs::write(&outside, b"private data").unwrap();
    let path = dir.path().join("ferrite.db");
    std::os::unix::fs::symlink(&outside, &path).unwrap();
    assert!(matches!(
        quarantine_database(&path),
        Err(RecoveryError::NotFile(_))
    ));
    assert!(path.is_symlink());
    assert_eq!(fs::read(outside).unwrap(), b"private data");
}

#[test]
fn a_missing_file_set_creates_no_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    assert_eq!(quarantine_database(&path).unwrap(), None);
    assert!(!path.exists());
}

#[test]
fn a_completed_quarantine_remains_discoverable_until_acknowledged() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    let first = quarantine_database(&path).unwrap().unwrap();
    let resumed = quarantine_database(&path).unwrap();
    assert_eq!(resumed, Some(first));
    super::acknowledge_quarantine(&path, 1).unwrap();
    assert_eq!(quarantine_database(&path).unwrap(), None);
    assert_eq!(
        fs::read(suffix(&path, ".corrupt-1")).unwrap(),
        b"damaged database"
    );
}

#[test]
fn an_old_receipt_never_quarantines_the_replacement_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    quarantine_database(&path).unwrap();
    fs::write(&path, b"verified replacement").unwrap();
    let resumed = quarantine_database(&path).unwrap().unwrap();
    assert_eq!(resumed.slot, 1);
    assert_eq!(fs::read(&path).unwrap(), b"verified replacement");
    assert!(super::acknowledge_quarantine(&path, 2).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"verified replacement");
}

#[cfg(windows)]
#[test]
fn a_database_held_without_delete_sharing_is_preserved_on_failure() {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ: u32 = 1;
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    let _held = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&path)
        .unwrap();
    assert!(quarantine_database(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"damaged database");
    assert!(suffix(&path, ".quarantine-pending").exists());
}

#[test]
fn a_crash_after_preserving_a_hard_link_resumes_without_manual_repair() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    let target = suffix(&path, ".corrupt-1");
    fs::hard_link(&path, &target).unwrap();
    fs::write(suffix(&path, ".quarantine-pending"), b"1 database\n").unwrap();
    let report = quarantine_database(&path).unwrap().unwrap();
    assert!(!path.exists());
    assert_eq!(report.database, Some(target.clone()));
    assert_eq!(fs::read(target).unwrap(), b"damaged database");
}

#[test]
fn unpublished_recovery_records_are_absent_and_do_not_block_a_retry() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    let record = suffix(&path, ".quarantine-pending");
    let _unpublished = super::record::prepare(
        &record,
        Pending {
            slot: 1,
            database: true,
        },
    )
    .unwrap();
    assert!(
        !record.exists(),
        "a record was published before the publication phase"
    );
    let report = quarantine_database(&path).unwrap().unwrap();
    assert_eq!(
        fs::read(report.database.unwrap()).unwrap(),
        b"damaged database"
    );
}

#[test]
fn separate_files_with_identical_bytes_are_preserved_as_an_ambiguous_move() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    let target = suffix(&path, ".corrupt-1");
    fs::write(&target, b"damaged database").unwrap();
    fs::write(suffix(&path, ".quarantine-pending"), b"1 database\n").unwrap();
    assert!(matches!(
        quarantine_database(&path),
        Err(RecoveryError::AmbiguousMove { .. })
    ));
    assert_eq!(fs::read(path).unwrap(), b"damaged database");
    assert_eq!(fs::read(target).unwrap(), b"damaged database");
}

#[test]
fn recovery_record_publication_never_replaces_an_existing_record() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pending");
    fs::write(&path, b"9 sidecars\n").unwrap();
    assert!(
        super::record::write(
            &path,
            Pending {
                slot: 1,
                database: true
            }
        )
        .is_err()
    );
    assert_eq!(fs::read(path).unwrap(), b"9 sidecars\n");
}

#[test]
fn an_empty_unpublished_temporary_cannot_poison_the_next_quarantine() {
    let dir = tempfile::tempdir().unwrap();
    let path = database(&dir);
    let orphan = tempfile::Builder::new()
        .prefix(".ferrite-quarantine-record-")
        .tempfile_in(dir.path())
        .unwrap()
        .into_temp_path()
        .keep()
        .unwrap();
    let report = quarantine_database(&path).unwrap().unwrap();
    assert_eq!(
        fs::read(report.database.unwrap()).unwrap(),
        b"damaged database"
    );
    assert!(orphan.exists());
    assert!(fs::read(orphan).unwrap().is_empty());
}

#[test]
#[ignore = "requires FERRITE_QUARANTINE_TEST_ROOT on an explicitly selected filesystem without hard links"]
fn quarantine_succeeds_on_a_native_filesystem_without_hard_links() {
    let root =
        std::env::var_os("FERRITE_QUARANTINE_TEST_ROOT").expect("select an isolated test root");
    let dir = tempfile::tempdir_in(root).unwrap();
    let path = database(&dir);
    assert!(
        fs::hard_link(&path, dir.path().join("link-probe")).is_err(),
        "the selected filesystem supports hard links"
    );
    let report = quarantine_database(&path).unwrap().unwrap();
    assert!(!path.exists());
    assert_eq!(
        fs::read(report.database.unwrap()).unwrap(),
        b"damaged database"
    );
}
