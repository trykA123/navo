use rusty_core::Location;
use rusty_fs::FakeFs;
use rusty_store::StoreDirs;

use super::operations;
use crate::jobs;

fn store(dir: &tempfile::TempDir) -> StoreDirs {
    StoreDirs {
        config: dir.path().join("config"),
        data: dir.path().join("data"),
    }
}

fn home() -> Location {
    Location::new("/home/admin").unwrap()
}

fn one_running_job(dirs: &StoreDirs) {
    dirs.ensure().unwrap();
    let journal = jobs::open(&dirs.data.join("ferrite.db")).unwrap();
    journal
        .lock()
        .unwrap()
        .create_job("copy", b"not a plan", 0, 0)
        .unwrap();
}

fn running_jobs(dirs: &StoreDirs) -> usize {
    let journal = jobs::open(&dirs.data.join("ferrite.db")).unwrap();
    let guard = journal.lock().unwrap();
    guard.unfinished().unwrap().len()
}

#[test]
fn without_store_folders_only_home_is_protected() {
    let (ops, lock) = operations(None, &home(), &FakeFs::new());
    assert_eq!(ops.protected, [home()]);
    assert!(lock.is_none());
}

#[test]
fn store_folders_are_protected_and_get_a_journal() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = store(&dir);
    let (ops, lock) = operations(Some(&dirs), &home(), &FakeFs::new());
    assert_eq!(ops.protected.len(), 3);
    assert!(dirs.data.join("ferrite.db").exists());
    assert!(lock.is_some());
}

#[test]
fn the_first_instance_closes_jobs_left_running() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = store(&dir);
    one_running_job(&dirs);
    let _started = operations(Some(&dirs), &home(), &FakeFs::new());
    assert_eq!(running_jobs(&dirs), 0);
}

#[test]
fn a_second_instance_leaves_the_first_ones_jobs_alone() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = store(&dir);
    let first = operations(Some(&dirs), &home(), &FakeFs::new());
    one_running_job(&dirs);
    let (_, second) = operations(Some(&dirs), &home(), &FakeFs::new());
    assert!(second.is_none());
    assert_eq!(running_jobs(&dirs), 1);
    drop(first);
}
