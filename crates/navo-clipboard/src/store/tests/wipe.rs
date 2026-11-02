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
