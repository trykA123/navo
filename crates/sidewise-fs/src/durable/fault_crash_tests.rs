//! Tests of `FaultDisk` crash images: pending changes that follow a durable folder rename,
//! `crash_with` subsets and torn tails, `FailFrom`, and unsynced `set_modified`/`copy_mode`.
// Linux only (`RealDisk` is built on Linux until D3), and only with the feature.
#![cfg(all(target_os = "linux", feature = "test-support"))]

use std::io::{ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::{Disk, FaultDisk, Plan, RealDisk, Tail};

fn put(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
}

fn text(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// A layer that passes every op, over a fresh temp folder.
fn pass() -> (tempfile::TempDir, FaultDisk) {
    (
        tempfile::tempdir().unwrap(),
        FaultDisk::new(RealDisk, Plan::Pass),
    )
}

#[test]
fn a_create_in_a_durably_renamed_folder_is_undone_at_its_new_place() {
    let (dir, disk) = pass();
    let (job, trash) = (dir.path().join("job"), dir.path().join("job.trash"));
    disk.create_dir(&job).unwrap();
    let mut file = disk.create_new(&job.join("log")).unwrap();
    file.write_all(b"x").unwrap();
    file.sync().unwrap();
    disk.rename_new(&job, &trash).unwrap();
    disk.sync_dir(dir.path()).unwrap();
    disk.crash().unwrap();
    assert!(trash.is_dir(), "the folder rename was durable");
    assert_eq!(
        text(&trash.join("log")),
        None,
        "its log entry was never synced"
    );
}

#[test]
fn undo_fails_when_a_created_file_vanished_outside_the_layer() {
    let (dir, disk) = pass();
    let path = dir.path().join("f");
    drop(disk.create_new(&path).unwrap());
    std::fs::remove_file(&path).unwrap();
    assert_eq!(disk.crash().unwrap_err().kind(), ErrorKind::NotFound);
}

#[test]
fn undo_fails_when_a_renamed_file_vanished_outside_the_layer() {
    let (dir, disk) = pass();
    let (from, to) = (dir.path().join("a"), dir.path().join("b"));
    put(&from, "x");
    disk.rename_new(&from, &to).unwrap();
    std::fs::remove_file(&to).unwrap();
    assert_eq!(disk.crash().unwrap_err().kind(), ErrorKind::NotFound);
}

#[test]
fn a_durable_remove_explains_an_older_create_that_is_gone() {
    let (dir, disk) = pass();
    let job = dir.path().join("job");
    disk.create_dir(&job).unwrap();
    drop(disk.create_new(&job.join("f")).unwrap());
    disk.remove_dir_all(&job).unwrap();
    disk.sync_dir(dir.path()).unwrap();
    disk.crash().unwrap();
    assert!(!job.exists());
}

#[test]
fn crash_with_can_keep_a_temp_and_lose_the_journal_tail_an_orphan_temp() {
    let (dir, disk) = pass();
    let (journal, temp) = (dir.path().join("journal"), dir.path().join("t.tmp"));
    put(&journal, "begin\n");
    let mut log = disk.open_append(&journal).unwrap();
    let mut file = disk.create_new(&temp).unwrap();
    file.write_all(b"data").unwrap();
    file.sync().unwrap();
    log.write_all(b"intent t.tmp\n").unwrap();
    assert_eq!(disk.unsynced_changes(), 1);
    disk.crash_with(|_| true, |_| Tail::Lost).unwrap();
    assert_eq!(text(&temp).as_deref(), Some("data"), "the create was kept");
    assert_eq!(
        text(&journal).as_deref(),
        Some("begin\n"),
        "the record was lost"
    );
}

#[test]
fn crash_with_a_torn_tail_keeps_the_first_half_of_the_unsynced_bytes() {
    let (dir, disk) = pass();
    let path = dir.path().join("log");
    put(&path, "old\n");
    let mut file = disk.open_append(&path).unwrap();
    file.write_all(b"abcd").unwrap();
    disk.crash_with(|_| true, |_| Tail::Torn).unwrap();
    assert_eq!(text(&path).as_deref(), Some("old\nab"));
}

#[test]
fn crash_with_keeps_only_the_chosen_changes_and_the_chosen_tails() {
    let (dir, disk) = pass();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    let mut fa = disk.create_new(&a).unwrap();
    let mut fb = disk.create_new(&b).unwrap();
    fa.write_all(b"aa").unwrap();
    fb.write_all(b"bb").unwrap();
    disk.crash_with(|i| i == 1, |id| [Tail::Lost, Tail::Kept][id])
        .unwrap();
    assert_eq!(text(&a), None);
    assert_eq!(text(&b).as_deref(), Some("bb"));
}

#[test]
fn a_kept_rename_carries_a_reverted_older_create_to_its_new_name() {
    let (dir, disk) = pass();
    let (staged, target) = (dir.path().join("s"), dir.path().join("t"));
    drop(disk.create_new(&staged).unwrap());
    disk.rename_new(&staged, &target).unwrap();
    disk.crash_with(|i| i == 1, |_| Tail::Lost).unwrap();
    assert_eq!(text(&staged), None);
    assert_eq!(
        text(&target),
        None,
        "the create was lost, so nothing was there to move"
    );
}

#[test]
fn crash_still_reverts_every_change_after_crash_with_exists() {
    let (dir, disk) = pass();
    let path = dir.path().join("f");
    drop(disk.create_new(&path).unwrap());
    disk.crash().unwrap();
    assert_eq!(text(&path), None);
}

#[test]
fn fail_from_fails_op_n_and_every_later_op_with_its_kind() {
    let (dir, disk) = (
        tempfile::tempdir().unwrap(),
        FaultDisk::new(RealDisk, Plan::FailFrom(2, ErrorKind::PermissionDenied)),
    );
    assert!(disk.list(dir.path()).is_ok());
    for _ in 0..3 {
        let error = disk.create_dir(&dir.path().join("d")).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::PermissionDenied);
    }
    assert!(!dir.path().join("d").exists());
}

#[test]
fn an_unsynced_set_modified_is_undone_by_crash() {
    let (dir, disk) = pass();
    let path = dir.path().join("f");
    put(&path, "x");
    RealDisk.set_modified(&path, 1_000_000_000_000).unwrap();
    disk.set_modified(&path, 1_500_000_000_000).unwrap();
    disk.crash().unwrap();
    let meta = RealDisk.meta(&path).unwrap().unwrap();
    assert_eq!(meta.modified_ms, Some(1_000_000_000_000));
}

#[test]
fn a_set_modified_followed_by_a_file_sync_survives_crash() {
    let (dir, disk) = pass();
    let path = dir.path().join("f");
    put(&path, "x");
    let mut file = disk.open_append(&path).unwrap();
    disk.set_modified(&path, 1_500_000_000_000).unwrap();
    file.sync().unwrap();
    disk.crash().unwrap();
    let meta = RealDisk.meta(&path).unwrap().unwrap();
    assert_eq!(meta.modified_ms, Some(1_500_000_000_000));
}

#[test]
fn an_unsynced_copy_mode_is_undone_by_crash() {
    let (dir, disk) = pass();
    let (from, to) = (dir.path().join("a"), dir.path().join("b"));
    put(&from, "a");
    put(&to, "b");
    std::fs::set_permissions(&from, std::fs::Permissions::from_mode(0o640)).unwrap();
    std::fs::set_permissions(&to, std::fs::Permissions::from_mode(0o600)).unwrap();
    disk.copy_mode(&from, &to).unwrap();
    disk.crash().unwrap();
    let mode = std::fs::metadata(&to).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn a_kept_create_settles_an_older_remove_at_its_path() {
    let (dir, disk) = pass();
    let path = dir.path().join("x");
    put(&path, "old");
    disk.remove_file(&path).unwrap();
    drop(disk.create_new(&path).unwrap());
    disk.crash_with(|i| i == 1, |_| Tail::Kept).unwrap();
    assert_eq!(
        text(&path).as_deref(),
        Some(""),
        "the new file stays, so the remove before it happened too"
    );
}

#[test]
fn a_kept_rename_keeps_the_older_rename_that_freed_its_target() {
    let (dir, disk) = pass();
    let (s, t, old) = (
        dir.path().join("s"),
        dir.path().join("t"),
        dir.path().join("old"),
    );
    put(&s, "S");
    put(&t, "T");
    disk.rename_new(&t, &old).unwrap();
    disk.rename_new(&s, &t).unwrap();
    disk.crash_with(|i| i == 1, |_| Tail::Lost).unwrap();
    assert_eq!(text(&s), None);
    assert_eq!(text(&t).as_deref(), Some("S"));
    assert_eq!(
        text(&old).as_deref(),
        Some("T"),
        "a kept change keeps the older changes on its paths, although keep(0) is false"
    );
}

#[test]
fn a_durable_rename_into_t_leaves_an_older_move_out_of_t_undoable() {
    let (dir, disk) = pass();
    let (a, other) = (dir.path().join("a"), dir.path().join("other"));
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&other).unwrap();
    let (s, t, old) = (a.join("s"), a.join("t"), other.join("old"));
    put(&s, "S");
    put(&t, "T");
    disk.rename_new(&t, &old).unwrap();
    disk.rename_new(&s, &t).unwrap();
    disk.sync_dir(&a).unwrap();
    disk.crash().unwrap();
    assert_eq!(
        text(&t).as_deref(),
        Some("S"),
        "the rename into t was durable"
    );
    assert_eq!(
        text(&old),
        None,
        "the entry in the unsynced folder may be lost, and with it the old t"
    );
}
