use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::safety::{assert_absent_everywhere, db_path, names, noise, with_suffix};
use super::text;
use crate::limits::Limits;
use crate::store::{ClipError, ClipStore, Opened};

const PAGE: usize = 4096;
const CANARY: &str = "canary-in-a-damaged-copy";

fn damaged_with_canary(path: &Path) {
    let mut bytes = noise(3 * PAGE);
    bytes[100..100 + CANARY.len()].copy_from_slice(CANARY.as_bytes());
    fs::write(path, bytes).unwrap();
    fs::write(with_suffix(path, "-wal"), CANARY).unwrap();
    fs::write(with_suffix(path, "-shm"), CANARY).unwrap();
}

fn open_after_recovery(dir: &TempDir) -> ClipStore {
    let path = db_path(dir);
    damaged_with_canary(&path);
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    assert!(matches!(store.opened(), Opened::MovedAside(_)));
    store
}

#[test]
fn wipe_all_also_removes_the_set_aside_copies_of_damaged_files() {
    let dir = TempDir::new().unwrap();
    let store = open_after_recovery(&dir);
    for name in [
        "clipboard.db.corrupt-7-1",
        "clipboard.db.corrupt-7-1-wal",
        "clipboard.db.corrupt-7-1-shm",
    ] {
        fs::write(dir.path().join(name), CANARY).unwrap();
    }
    assert_eq!(
        names(dir.path())
            .iter()
            .filter(|name| name.contains(".corrupt-"))
            .count(),
        6
    );
    let store = store.wipe_all(&db_path(&dir)).unwrap();
    assert_absent_everywhere(dir.path(), CANARY);
    for name in names(dir.path()) {
        assert!(!name.contains(".corrupt-"), "wipe_all left {name}");
    }
    assert_eq!(store.opened(), &Opened::Clean);
}

#[test]
fn wipe_all_keeps_files_that_are_not_aside_copies_of_this_database() {
    let dir = TempDir::new().unwrap();
    let store = open_after_recovery(&dir);
    let others = [
        "clipboard.db.corrupt-abc",
        "clipboard.db.corrupt-",
        "clipboard.db.corrupt-1-",
        "clipboard.db.corrupt-1-2-3",
        "clipboard.db.corrupt-1-bak",
        "clipboard.db.corrupt-1.bak",
        "clipboard.db.corrupt-1-wal-old",
        "other.db.corrupt-1",
        "other.db.corrupt-1-wal",
        "xclipboard.db.corrupt-1",
        "clipboard.db.bak",
    ];
    for name in others {
        fs::write(dir.path().join(name), b"not ours").unwrap();
    }
    store.wipe_all(&db_path(&dir)).unwrap();
    for name in others {
        assert_eq!(
            fs::read(dir.path().join(name)).unwrap(),
            b"not ours",
            "{name}"
        );
    }
}

#[test]
fn an_aside_that_cannot_be_removed_is_an_error_and_the_live_database_stays() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let mut store = ClipStore::open(&path, Limits::default()).unwrap();
    store.record(&text("live row", 1)).unwrap();
    fs::create_dir(dir.path().join("clipboard.db.corrupt-5-wal")).unwrap();
    fs::write(dir.path().join("clipboard.db.corrupt-5"), CANARY).unwrap();
    let error = store.wipe_all(&path).err().unwrap();
    assert!(matches!(error, ClipError::Io(_)));
    assert!(!error.to_string().contains("clipboard.db"));
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    assert_eq!(store.history(10).unwrap().len(), 1);
    assert!(dir.path().join("clipboard.db.corrupt-5-wal").is_dir());
}

#[cfg(unix)]
#[test]
fn wipe_all_unlinks_an_aside_symlink_and_leaves_its_target() {
    let outside = TempDir::new().unwrap();
    let target = outside.path().join("precious");
    fs::write(&target, b"keep me").unwrap();
    let dir = TempDir::new().unwrap();
    let store = ClipStore::open(&db_path(&dir), Limits::default()).unwrap();
    let link = dir.path().join("clipboard.db.corrupt-9");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    store.wipe_all(&db_path(&dir)).unwrap();
    assert!(fs::symlink_metadata(&link).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"keep me");
}

#[cfg(unix)]
#[test]
fn wipe_all_leaves_a_file_with_a_non_utf8_name_alone() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let dir = TempDir::new().unwrap();
    let store = ClipStore::open(&db_path(&dir), Limits::default()).unwrap();
    let odd = dir
        .path()
        .join(OsStr::from_bytes(b"clipboard.db.corrupt-\xff"));
    fs::write(&odd, b"not ours").unwrap();
    store.wipe_all(&db_path(&dir)).unwrap();
    assert_eq!(fs::read(&odd).unwrap(), b"not ours");
}

fn staged(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join("clipboard.db.new")
}

fn store_with_row_and_aside(dir: &TempDir) -> ClipStore {
    let mut store = ClipStore::open(&db_path(dir), Limits::default()).unwrap();
    store.record(&text("live row", 1)).unwrap();
    fs::write(dir.path().join("clipboard.db.corrupt-5"), CANARY).unwrap();
    store
}

fn assert_nothing_deleted(dir: &TempDir) {
    assert!(!staged(dir).exists(), "no staged file stays");
    let aside = fs::read(dir.path().join("clipboard.db.corrupt-5")).unwrap();
    assert_eq!(aside, CANARY.as_bytes());
    let store = ClipStore::open(&db_path(dir), Limits::default()).unwrap();
    assert_eq!(store.history(10).unwrap().len(), 1);
}

#[test]
fn a_failed_seed_of_the_fresh_database_deletes_nothing() {
    let dir = TempDir::new().unwrap();
    let store = store_with_row_and_aside(&dir);
    super::set_fail_seed(true);
    let result = store.wipe_all(&db_path(&dir));
    super::set_fail_seed(false);
    assert!(result.is_err());
    assert_nothing_deleted(&dir);
}

#[test]
fn an_unwritable_staged_name_deletes_nothing() {
    let dir = TempDir::new().unwrap();
    let store = store_with_row_and_aside(&dir);
    fs::create_dir(staged(&dir)).unwrap();
    assert!(matches!(
        store.wipe_all(&db_path(&dir)),
        Err(ClipError::Io(_))
    ));
    fs::remove_dir(staged(&dir)).unwrap();
    assert_nothing_deleted(&dir);
}

#[test]
fn a_successful_wipe_leaves_no_staged_file_and_replaces_a_stale_one() {
    let dir = TempDir::new().unwrap();
    let store = store_with_row_and_aside(&dir);
    fs::write(staged(&dir), CANARY).unwrap();
    let store = store.wipe_all(&db_path(&dir)).unwrap();
    assert!(store.history(10).unwrap().is_empty());
    assert!(names(dir.path()).iter().all(|name| !name.contains(".new")));
    assert_absent_everywhere(dir.path(), CANARY);
}

#[test]
fn a_staged_file_left_without_a_database_is_opened_in_its_place() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let mut store = ClipStore::open(&path, Limits::default()).unwrap();
    for at in 1..=3 {
        store.record(&text(&format!("row {at}"), at)).unwrap();
    }
    let newest = store.history(1).unwrap()[0].id;
    drop(store.wipe_all(&path).unwrap());
    fs::rename(&path, staged(&dir)).unwrap();
    let mut store = ClipStore::open(&path, Limits::default()).unwrap();
    assert!(!staged(&dir).exists());
    store.record(&text("after", 9)).unwrap();
    assert!(
        store.history(1).unwrap()[0].id > newest,
        "the marks came along"
    );
}

#[test]
fn a_staged_file_is_ignored_beside_a_database_or_without_a_header() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    fs::write(staged(&dir), b"not a database").unwrap();
    let mut store = ClipStore::open(&path, Limits::default()).unwrap();
    assert!(store.history(10).unwrap().is_empty());
    assert_eq!(fs::read(staged(&dir)).unwrap(), b"not a database");
    store.record(&text("live", 1)).unwrap();
    drop(store);
    fs::copy(&path, staged(&dir)).unwrap();
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    assert_eq!(store.history(10).unwrap().len(), 1);
    assert!(staged(&dir).exists(), "a live database is never replaced");
}
