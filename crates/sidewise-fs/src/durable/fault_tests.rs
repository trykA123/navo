//! Tests of `FaultDisk`: op numbering, `FailAt` and `CrashAt`, and what `crash()` undoes.
// Linux only (`RealDisk` is built on Linux until D3), and only with the feature.
#![cfg(all(target_os = "linux", feature = "test-support"))]

use std::io::{ErrorKind, Write};
use std::path::Path;

use super::{Disk, FaultDisk, Plan, RealDisk};

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
fn a_write_without_sync_is_lost_after_crash() {
    let (dir, disk) = pass();
    let path = dir.path().join("f");
    let mut file = disk.create_new(&path).unwrap();
    disk.sync_dir(dir.path()).unwrap();
    file.write_all(b"abc").unwrap();
    disk.crash().unwrap();
    assert_eq!(text(&path).as_deref(), Some(""));
}

#[test]
fn a_synced_write_survives_crash_but_the_unsynced_tail_does_not() {
    let (dir, disk) = pass();
    let path = dir.path().join("f");
    let mut file = disk.create_new(&path).unwrap();
    disk.sync_dir(dir.path()).unwrap();
    file.write_all(b"abc").unwrap();
    file.sync().unwrap();
    file.write_all(b"def").unwrap();
    disk.crash().unwrap();
    assert_eq!(text(&path).as_deref(), Some("abc"));
}

#[test]
fn an_append_to_an_old_file_loses_only_the_unsynced_bytes() {
    let (dir, disk) = pass();
    let path = dir.path().join("log");
    put(&path, "old\n");
    let mut file = disk.open_append(&path).unwrap();
    file.write_all(b"new\n").unwrap();
    disk.crash().unwrap();
    assert_eq!(text(&path).as_deref(), Some("old\n"));
}

#[test]
fn a_create_without_sync_dir_is_undone() {
    let (dir, disk) = pass();
    let path = dir.path().join("f");
    let mut file = disk.create_new(&path).unwrap();
    file.write_all(b"abc").unwrap();
    file.sync().unwrap();
    disk.create_dir(&dir.path().join("sub")).unwrap();
    disk.crash().unwrap();
    assert_eq!(text(&path), None);
    assert!(!dir.path().join("sub").exists());
}

#[test]
fn a_rename_without_sync_dir_reverts() {
    let (dir, disk) = pass();
    let (from, to) = (dir.path().join("a"), dir.path().join("b"));
    put(&from, "x");
    disk.rename_new(&from, &to).unwrap();
    disk.crash().unwrap();
    assert_eq!(text(&from).as_deref(), Some("x"));
    assert_eq!(text(&to), None);
}

#[test]
fn a_rename_with_sync_dir_survives() {
    let (dir, disk) = pass();
    let (from, to) = (dir.path().join("a"), dir.path().join("b"));
    put(&from, "x");
    disk.rename_new(&from, &to).unwrap();
    disk.sync_dir(dir.path()).unwrap();
    disk.crash().unwrap();
    assert_eq!(text(&from), None);
    assert_eq!(text(&to).as_deref(), Some("x"));
}

#[test]
fn a_rename_across_folders_needs_both_folders_synced() {
    let (dir, disk) = pass();
    let other = dir.path().join("other");
    std::fs::create_dir(&other).unwrap();
    let (from, to) = (dir.path().join("a"), other.join("a"));
    put(&from, "x");
    disk.rename_new(&from, &to).unwrap();
    disk.sync_dir(&other).unwrap();
    disk.crash().unwrap();
    assert_eq!(text(&from).as_deref(), Some("x"));
    assert_eq!(text(&to), None);
}

#[test]
fn an_unsynced_file_keeps_losing_its_bytes_after_a_durable_rename() {
    let (dir, disk) = pass();
    let (staged, target) = (dir.path().join("s"), dir.path().join("t"));
    let mut file = disk.create_new(&staged).unwrap();
    file.write_all(b"new").unwrap();
    disk.rename_new(&staged, &target).unwrap();
    disk.sync_dir(dir.path()).unwrap();
    disk.crash().unwrap();
    assert_eq!(text(&target).as_deref(), Some(""));
}

#[test]
fn a_file_in_a_renamed_folder_is_still_cut_to_its_synced_length() {
    let (dir, disk) = pass();
    let (job, trash) = (dir.path().join("job"), dir.path().join("job.trash"));
    std::fs::create_dir(&job).unwrap();
    let mut file = disk.create_new(&job.join("log")).unwrap();
    disk.sync_dir(&job).unwrap();
    file.write_all(b"unsynced").unwrap();
    disk.rename_new(&job, &trash).unwrap();
    disk.sync_dir(dir.path()).unwrap();
    disk.crash().unwrap();
    assert_eq!(text(&trash.join("log")).as_deref(), Some(""));
}

#[test]
fn a_synced_staged_file_replaced_over_the_target_survives() {
    let (dir, disk) = pass();
    let (staged, target, old) = (
        dir.path().join("s"),
        dir.path().join("t"),
        dir.path().join("o"),
    );
    put(&target, "old");
    let mut file = disk.create_new(&staged).unwrap();
    file.write_all(b"new").unwrap();
    file.sync().unwrap();
    disk.replace(&staged, &target, &old).unwrap();
    disk.crash().unwrap();
    assert_eq!(text(&target).as_deref(), Some("new"));
    assert_eq!(text(&staged), None);
}

#[test]
fn a_remove_without_sync_dir_comes_back_with_its_bytes_and_time() {
    let (dir, disk) = pass();
    let path = dir.path().join("f");
    put(&path, "keep");
    RealDisk.set_modified(&path, 1_000_000_000_000).unwrap();
    disk.remove_file(&path).unwrap();
    disk.crash().unwrap();
    assert_eq!(text(&path).as_deref(), Some("keep"));
    let meta = RealDisk.meta(&path).unwrap().unwrap();
    assert_eq!(meta.modified_ms, Some(1_000_000_000_000));
}

#[test]
fn a_removed_tree_comes_back_unless_its_parent_was_synced() {
    let (dir, disk) = pass();
    let job = dir.path().join("job");
    std::fs::create_dir_all(job.join("deep")).unwrap();
    put(&job.join("log"), "l");
    put(&job.join("deep").join("1.bak"), "b");
    disk.remove_dir_all(&job).unwrap();
    disk.crash().unwrap();
    assert_eq!(text(&job.join("log")).as_deref(), Some("l"));
    assert_eq!(text(&job.join("deep").join("1.bak")).as_deref(), Some("b"));

    let disk = FaultDisk::new(RealDisk, Plan::Pass);
    disk.remove_dir_all(&job).unwrap();
    disk.sync_dir(dir.path()).unwrap();
    disk.crash().unwrap();
    assert!(!job.exists());
}

/// Runs a fixed script of ten ops; returns the 1-based number and kind of the first failure.
fn script(dir: &Path, disk: &FaultDisk) -> Option<(u64, ErrorKind)> {
    let path = dir.join("f");
    let mut step = 0;
    let mut fail = |result: std::io::Result<()>| {
        step += 1;
        result.err().map(|error| (step, error.kind()))
    };
    let mut file = match disk.create_new(&path) {
        Ok(file) => file,
        Err(error) => return Some((1, error.kind())),
    };
    fail(Ok(()))
        .or_else(|| fail(file.write_all(b"abc")))
        .or_else(|| fail(file.sync()))
        .or_else(|| fail(disk.sync_dir(dir)))
        .or_else(|| fail(disk.meta(&path).map(drop)))
        .or_else(|| fail(disk.set_modified(&path, 1)))
        .or_else(|| fail(disk.list(dir).map(drop)))
        .or_else(|| fail(disk.rename_new(&path, &dir.join("g"))))
        .or_else(|| fail(disk.sync_dir(dir)))
        .or_else(|| fail(disk.remove_file(&dir.join("g"))))
}

#[test]
fn ops_counts_every_disk_call_and_every_write_and_sync() {
    let (dir, disk) = pass();
    assert_eq!(script(dir.path(), &disk), None);
    assert_eq!(disk.ops(), 10);
}

#[test]
fn fail_at_hits_exactly_op_n_and_only_that_op() {
    for n in 1..=10 {
        let dir = tempfile::tempdir().unwrap();
        let disk = FaultDisk::new(RealDisk, Plan::FailAt(n, ErrorKind::StorageFull));
        assert_eq!(
            script(dir.path(), &disk),
            Some((n, ErrorKind::StorageFull)),
            "op {n}"
        );
        assert!(disk.list(dir.path()).is_ok(), "op after {n} must pass");
    }
}

#[test]
fn crash_at_fails_op_n_and_every_later_op() {
    for n in 1..=10 {
        let dir = tempfile::tempdir().unwrap();
        let disk = FaultDisk::new(RealDisk, Plan::CrashAt(n));
        assert_eq!(script(dir.path(), &disk).map(|(at, _)| at), Some(n));
        assert!(disk.list(dir.path()).is_err(), "op after {n} must fail");
        disk.crash().unwrap();
    }
}

#[test]
fn a_crash_at_every_op_leaves_no_file_or_only_whole_durable_ones() {
    for n in 1..=10 {
        let dir = tempfile::tempdir().unwrap();
        let disk = FaultDisk::new(RealDisk, Plan::CrashAt(n));
        script(dir.path(), &disk);
        disk.crash().unwrap();
        let names = RealDisk.list(dir.path()).unwrap();
        let whole = |name: &str| text(&dir.path().join(name)).as_deref() == Some("abc");
        let ok = match n {
            1..=4 => names.is_empty(),
            _ => names.len() == 1 && whole(&names[0]),
        };
        assert!(ok, "crash at op {n} left {names:?}");
    }
}

#[test]
fn ops_after_crash_fail_so_a_stale_handle_cannot_write() {
    let (dir, disk) = pass();
    let mut file = disk.create_new(&dir.path().join("f")).unwrap();
    disk.crash().unwrap();
    assert!(file.write_all(b"late").is_err());
    assert!(disk.list(dir.path()).is_err());
}
