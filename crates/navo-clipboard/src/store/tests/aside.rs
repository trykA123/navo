use std::fs;

use tempfile::TempDir;

use super::safety::{db_path, names, with_suffix};
use crate::limits::Limits;
use crate::store::safety::{Rename, absent_live, move_aside, move_no_replace, restore};
use crate::store::{ClipError, ClipStore};

const NEAR_MISSES: [&str; 12] = [
    "clipboard.db.corrupt-42-100",
    "clipboard.db.corrupt-42-100-wal",
    "clipboard.db.corrupt-42-0",
    "clipboard.db.corrupt-42-01",
    "clipboard.db.corrupt-42-01-shm",
    "clipboard.db.corrupt-042",
    "clipboard.db.corrupt-00",
    "clipboard.db.corrupt-+42",
    "clipboard.db.corrupt-42-+1",
    "clipboard.db.corrupt-340282366920938463463374607431768211456",
    "clipboard.db.corrupt-0340282366920938463463374607431768211455",
    "clipboard.db.corrupt-42-wal-wal",
];

const EXACT: [&str; 5] = [
    "clipboard.db.corrupt-0",
    "clipboard.db.corrupt-42-99",
    "clipboard.db.corrupt-42-1-shm",
    "clipboard.db.corrupt-340282366920938463463374607431768211455-wal",
    "clipboard.db.corrupt-7-12",
];

#[test]
fn wipe_all_keeps_names_the_generator_can_never_make() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    for name in NEAR_MISSES.iter().chain(&EXACT) {
        fs::write(dir.path().join(name), b"x").unwrap();
    }
    store.wipe_all(&path).unwrap();
    for name in NEAR_MISSES {
        assert!(dir.path().join(name).exists(), "{name} must stay");
    }
    for name in EXACT {
        assert!(!dir.path().join(name).exists(), "{name} must go");
    }
}

#[test]
fn every_name_the_generator_makes_is_removed_by_wipe_all() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    for _ in 0..100 {
        fs::write(&path, b"damaged").unwrap();
        fs::write(with_suffix(&path, "-wal"), b"damaged wal").unwrap();
        move_aside(&path, u128::MAX).unwrap();
    }
    fs::write(&path, b"damaged").unwrap();
    assert!(move_aside(&path, u128::MAX).is_err());
    fs::remove_file(&path).unwrap();
    assert_eq!(names(dir.path()).len(), 200);
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    store.wipe_all(&path).unwrap();
    for name in names(dir.path()) {
        assert!(!name.contains(".corrupt-"), "wipe_all left {name}");
    }
}

#[cfg(unix)]
#[test]
fn a_dangling_symlink_at_the_first_free_name_is_never_replaced() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    fs::write(&path, b"damaged").unwrap();
    let link = dir.path().join("clipboard.db.corrupt-42");
    std::os::unix::fs::symlink(dir.path().join("nowhere"), &link).unwrap();
    let moved = move_aside(&path, 42).unwrap().aside;
    assert_eq!(moved, dir.path().join("clipboard.db.corrupt-42-1"));
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&moved).unwrap(), b"damaged");
}

#[cfg(unix)]
#[test]
fn the_move_refuses_a_dangling_symlink_that_appears_after_the_check() {
    let dir = TempDir::new().unwrap();
    let from = dir.path().join("from");
    let to = dir.path().join("to");
    fs::write(&from, b"data").unwrap();
    std::os::unix::fs::symlink(dir.path().join("nowhere"), &to).unwrap();
    let error = move_no_replace(&from, &to).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&from).unwrap(), b"data");
    assert!(fs::symlink_metadata(&to).unwrap().file_type().is_symlink());
}

#[test]
fn the_move_refuses_an_existing_file_and_keeps_both() {
    let dir = TempDir::new().unwrap();
    let from = dir.path().join("from");
    let to = dir.path().join("to");
    fs::write(&from, b"data").unwrap();
    fs::write(&to, b"taken").unwrap();
    assert!(move_no_replace(&from, &to).is_err());
    assert_eq!(fs::read(&from).unwrap(), b"data");
    assert_eq!(fs::read(&to).unwrap(), b"taken");
    move_no_replace(&from, &dir.path().join("free")).unwrap();
    assert_eq!(names(dir.path()), ["free", "to"]);
}

#[test]
fn a_failing_restore_rename_is_reported() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let absent = absent_live(&path);
    let renames = [Rename {
        from: path.clone(),
        to: dir.path().join("clipboard.db.corrupt-1"),
    }];
    let error = restore(&path, &renames, &absent).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn restore_removes_only_files_the_failed_open_created() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let aside = dir.path().join("clipboard.db.corrupt-1");
    fs::write(&aside, b"damaged").unwrap();
    fs::write(with_suffix(&path, "-shm"), b"not ours").unwrap();
    let absent = absent_live(&path);
    fs::write(&path, b"fresh").unwrap();
    fs::write(with_suffix(&path, "-wal"), b"fresh wal").unwrap();
    let renames = [Rename {
        from: path.clone(),
        to: aside.clone(),
    }];
    restore(&path, &renames, &absent).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"damaged");
    assert!(!with_suffix(&path, "-wal").exists());
    assert_eq!(fs::read(with_suffix(&path, "-shm")).unwrap(), b"not ours");
}

#[test]
fn restore_never_replaces_a_live_file_that_was_there_before() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let aside = dir.path().join("clipboard.db.corrupt-1");
    fs::write(&aside, b"damaged").unwrap();
    fs::write(&path, b"someone else's").unwrap();
    let absent = absent_live(&path);
    let renames = [Rename {
        from: path.clone(),
        to: aside.clone(),
    }];
    assert!(restore(&path, &renames, &absent).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"someone else's");
    assert_eq!(fs::read(&aside).unwrap(), b"damaged");
}

#[test]
fn an_incomplete_restore_names_no_path() {
    let error = ClipError::RestoreIncomplete {
        source: Box::new(ClipError::Corrupt),
    };
    assert!(!error.to_string().contains("clipboard.db"));
    assert!(!error.is_corruption());
}
