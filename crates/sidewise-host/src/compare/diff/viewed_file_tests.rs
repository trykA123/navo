//! Tests for the file-backed Viewed store.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use super::*;

const KEY_1: [u8; 64] = [1; 64];
const KEY_2: [u8; 64] = [2; 64];
const KEY_3: [u8; 64] = [3; 64];

/// The log path under `sub`, whose folder does not exist yet.
fn log_path(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("sub").join("viewed.log")
}

#[test]
fn marks_survive_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = log_path(&dir);
    {
        let store = FileViewed::open(&path).unwrap();
        store.set(&KEY_1, true).unwrap();
        store.set(&KEY_2, true).unwrap();
        store.set(&KEY_2, false).unwrap();
    }
    let store = FileViewed::open(&path).unwrap();
    assert!(store.get(&KEY_1).unwrap());
    assert!(!store.get(&KEY_2).unwrap());
    assert_eq!(store.len(), 1);
}

#[test]
fn a_missing_file_opens_empty_and_creates_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = log_path(&dir);
    let store = FileViewed::open(&path).unwrap();
    assert!(store.is_empty());
    assert!(!path.exists());
}

#[test]
fn clear_all_removes_every_mark_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = log_path(&dir);
    {
        let store = FileViewed::open(&path).unwrap();
        store.set(&KEY_1, true).unwrap();
        store.set(&KEY_2, true).unwrap();
        store.set(&KEY_3, true).unwrap();
        assert_eq!(store.clear_all().unwrap(), 3);
    }
    let store = FileViewed::open(&path).unwrap();
    assert!(store.is_empty());
}

#[test]
fn a_torn_last_record_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let path = log_path(&dir);
    {
        let store = FileViewed::open(&path).unwrap();
        store.set(&KEY_1, true).unwrap();
    }
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(&[0xAB; 10]).unwrap();
    drop(file);
    let store = FileViewed::open(&path).unwrap();
    assert!(store.get(&KEY_1).unwrap());
    assert_eq!(fs::metadata(&path).unwrap().len(), 65);
}

#[test]
fn a_bad_flag_stops_the_replay() {
    let dir = tempfile::tempdir().unwrap();
    let path = log_path(&dir);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut bytes = Vec::new();
    bytes.push(1);
    bytes.extend_from_slice(&KEY_1);
    bytes.push(7);
    bytes.extend_from_slice(&KEY_2);
    bytes.push(1);
    bytes.extend_from_slice(&KEY_2);
    fs::write(&path, &bytes).unwrap();
    let store = FileViewed::open(&path).unwrap();
    assert!(store.get(&KEY_1).unwrap());
    assert!(!store.get(&KEY_2).unwrap());
    assert_eq!(fs::metadata(&path).unwrap().len(), 65);
}

#[test]
fn repeated_toggles_compact_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = log_path(&dir);
    {
        let store = FileViewed::open(&path).unwrap();
        for _ in 0..600 {
            store.set(&KEY_1, true).unwrap();
            store.set(&KEY_1, false).unwrap();
        }
        store.set(&KEY_2, true).unwrap();
    }
    let store = FileViewed::open(&path).unwrap();
    assert!(store.get(&KEY_2).unwrap());
    assert!(!store.get(&KEY_1).unwrap());
    assert_eq!(fs::metadata(&path).unwrap().len(), 65);
}
