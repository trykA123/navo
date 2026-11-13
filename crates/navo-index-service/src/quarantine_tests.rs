//! Real temporary-file checks for marker classification, cleanup and persistence faults.
#![allow(
    clippy::unwrap_used,
    reason = "test setup and filesystem operations must succeed"
)]
use super::*;
const ID: Identity = Identity {
    serial: 7,
    journal: 8,
    cursor: 9,
    checksum: 10,
};

#[test]
fn quarantine_absent_and_complete() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("C.index");
    assert_eq!(read(&path, ID).unwrap(), Marker::Absent);
    write_complete(&path, ID).unwrap();
    assert_eq!(read(&path, ID).unwrap(), Marker::Complete);
}
#[test]
fn quarantine_active_torn_and_unknown_are_search_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("C.index");
    write_active(&path, ID).unwrap();
    assert_eq!(read(&path, ID).unwrap(), Marker::SearchOnly);
    for bytes in [
        "NAVO-QUARANTINE-1:complete:7",
        "unknown",
        "",
        &"x".repeat(10000),
    ] {
        fs::write(sibling(&path, ".rebuilding"), bytes).unwrap();
        assert_eq!(read(&path, ID).unwrap(), Marker::SearchOnly);
    }
}
#[test]
fn quarantine_complete_requires_matching_serial_and_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("C.index");
    write_complete(&path, ID).unwrap();
    for id in [
        Identity { serial: 6, ..ID },
        Identity { journal: 6, ..ID },
        Identity { cursor: 6, ..ID },
        Identity { checksum: 6, ..ID },
    ] {
        assert_eq!(read(&path, id).unwrap(), Marker::SearchOnly);
    }
}
#[test]
fn quarantine_complete_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("C.index");
    write_active(&path, ID).unwrap();
    assert!(clear(&path, ID).is_err());
    write_complete(&path, ID).unwrap();
    clear(&path, ID).unwrap();
    clear(&path, ID).unwrap();
    assert_eq!(read(&path, ID).unwrap(), Marker::Absent);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}
#[test]
fn quarantine_sync_replace_and_flush_fail_closed() {
    for at in 1..=3 {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("C.index");
        write_active(&path, ID).unwrap();
        let result = write_with(&path, &record("complete", ID), &mut |step| {
            if step == at {
                Err(io::ErrorKind::PermissionDenied.into())
            } else {
                Ok(())
            }
        });
        assert!(
            result.is_err(),
            "marker persistence failure must be returned"
        );
        assert_eq!(read(&path, ID).unwrap(), Marker::SearchOnly);
    }
}
