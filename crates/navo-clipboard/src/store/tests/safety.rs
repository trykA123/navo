use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use rusqlite::config::DbConfig;
use tempfile::TempDir;

use super::text;
use crate::limits::Limits;
use crate::store::safety::{Rename, move_aside, rename_all, rename_if_free};
use crate::store::{ClipError, ClipStore, Opened};

const PAGE: usize = 4096;

pub(super) fn db_path(dir: &TempDir) -> PathBuf {
    dir.path().join("clipboard.db")
}

pub(super) fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

pub(super) fn noise(len: usize) -> Vec<u8> {
    let mut state: u32 = 0x1234_5678;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            state.to_be_bytes()[0]
        })
        .collect()
}

pub(super) fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

pub(super) fn assert_absent_everywhere(dir: &Path, canary: &str) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let bytes = fs::read(&path).unwrap();
        assert!(
            !contains(&bytes, canary.as_bytes()),
            "a cleared body must not survive in {}",
            path.display()
        );
    }
}

fn moved_aside(store: &ClipStore) -> PathBuf {
    match store.opened() {
        Opened::MovedAside(path) => path.clone(),
        Opened::Clean => panic!("a damaged file must be moved aside"),
    }
}

fn assert_usable(store: &mut ClipStore) {
    assert!(store.history(10).unwrap().is_empty());
    store.record(&text("after recovery", 1)).unwrap();
    assert_eq!(store.history(10).unwrap().len(), 1);
}

fn filled_database(path: &Path) {
    let mut store = ClipStore::open(path, Limits::default()).unwrap();
    for at in 0..300 {
        store
            .record(&text(&format!("row-{at}-{}", "x".repeat(200)), at))
            .unwrap();
    }
    drop(store);
    assert!(!with_suffix(path, "-wal").exists());
}

#[test]
fn a_healthy_file_opens_clean_and_keeps_its_rows() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let mut store = ClipStore::open(&path, Limits::default()).unwrap();
    store.record(&text("kept", 1)).unwrap();
    drop(store);
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    assert_eq!(store.opened(), &Opened::Clean);
    assert_eq!(store.history(10).unwrap().len(), 1);
}

#[test]
fn a_random_bytes_file_and_its_companions_are_moved_aside() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let original = noise(3 * PAGE);
    fs::write(&path, &original).unwrap();
    fs::write(with_suffix(&path, "-wal"), b"old wal").unwrap();
    fs::write(with_suffix(&path, "-shm"), b"old shm").unwrap();
    let mut store = ClipStore::open(&path, Limits::default()).unwrap();
    let aside = moved_aside(&store);
    assert_eq!(aside.parent(), Some(dir.path()));
    let name = aside.file_name().unwrap().to_string_lossy().into_owned();
    let stamp = name.strip_prefix("clipboard.db.corrupt-").unwrap();
    assert!(!stamp.is_empty() && stamp.bytes().all(|byte| byte.is_ascii_digit()));
    assert_eq!(fs::read(&aside).unwrap(), original);
    assert_eq!(fs::read(with_suffix(&aside, "-wal")).unwrap(), b"old wal");
    assert!(with_suffix(&aside, "-shm").exists());
    assert_usable(&mut store);
}

#[test]
fn a_damaged_page_is_found_by_quick_check_and_moved_aside() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    filled_database(&path);
    let mut bytes = fs::read(&path).unwrap();
    assert!(bytes.len() > 4 * PAGE);
    bytes[PAGE..2 * PAGE].copy_from_slice(&noise(PAGE));
    let damaged = bytes.clone();
    fs::write(&path, &bytes).unwrap();
    let mut store = ClipStore::open(&path, Limits::default()).unwrap();
    let aside = moved_aside(&store);
    assert_eq!(fs::read(&aside).unwrap(), damaged);
    assert_usable(&mut store);
}

#[test]
fn a_damaged_file_closes_without_checkpointing_its_wal() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    filled_database(&path);
    let conn = Connection::open(&path).unwrap();
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
        .unwrap();
    conn.execute_batch("CREATE TABLE extra (x); INSERT INTO extra VALUES ('in wal');")
        .unwrap();
    drop(conn);
    let wal = fs::read(with_suffix(&path, "-wal")).unwrap();
    assert!(!wal.is_empty());
    let mut bytes = fs::read(&path).unwrap();
    bytes[PAGE..2 * PAGE].copy_from_slice(&noise(PAGE));
    fs::write(&path, &bytes).unwrap();
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    let aside = moved_aside(&store);
    assert_eq!(fs::read(&aside).unwrap(), bytes);
    assert_eq!(fs::read(with_suffix(&aside, "-wal")).unwrap(), wal);
}

#[test]
fn a_truncated_database_is_moved_aside() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    filled_database(&path);
    let bytes = fs::read(&path).unwrap();
    let truncated = bytes[..bytes.len() / 2].to_vec();
    fs::write(&path, &truncated).unwrap();
    let mut store = ClipStore::open(&path, Limits::default()).unwrap();
    let aside = moved_aside(&store);
    assert_eq!(fs::read(&aside).unwrap(), truncated);
    assert_usable(&mut store);
}

#[test]
fn an_existing_aside_name_is_never_overwritten() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    fs::write(&path, b"damaged").unwrap();
    fs::write(with_suffix(&path, "-wal"), b"damaged wal").unwrap();
    fs::write(dir.path().join("clipboard.db.corrupt-42"), b"first").unwrap();
    fs::write(dir.path().join("clipboard.db.corrupt-42-1-wal"), b"second").unwrap();
    let moved = move_aside(&path, 42).unwrap().aside;
    assert_eq!(moved, dir.path().join("clipboard.db.corrupt-42-2"));
    assert_eq!(
        fs::read(dir.path().join("clipboard.db.corrupt-42")).unwrap(),
        b"first"
    );
    assert_eq!(
        fs::read(dir.path().join("clipboard.db.corrupt-42-1-wal")).unwrap(),
        b"second"
    );
    assert_eq!(fs::read(&moved).unwrap(), b"damaged");
    assert_eq!(
        fs::read(with_suffix(&moved, "-wal")).unwrap(),
        b"damaged wal"
    );
    assert!(!path.exists());
}

#[test]
fn a_newer_version_file_is_refused_and_left_untouched() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE future (x); INSERT INTO future VALUES ('keep');")
        .unwrap();
    conn.pragma_update(None, "user_version", 99).unwrap();
    drop(conn);
    let before = fs::read(&path).unwrap();
    let listing = names(dir.path());
    let result = ClipStore::open(&path, Limits::default());
    assert!(matches!(result, Err(ClipError::TooNew { found: 99, .. })));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(names(dir.path()), listing);
}

#[test]
fn clear_history_and_wipe_all_leave_no_canary_on_disk() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let mut store = ClipStore::open(&path, Limits::default()).unwrap();
    store
        .record(&text("canary-cleared-by-clear-history", 1))
        .unwrap();
    store.clear_history().unwrap();
    assert_absent_everywhere(dir.path(), "canary-cleared-by-clear-history");
    store
        .record(&text("canary-removed-by-wipe-all", 2))
        .unwrap();
    let store = store.wipe_all(&path).unwrap();
    assert_absent_everywhere(dir.path(), "canary-removed-by-wipe-all");
    assert!(store.history(10).unwrap().is_empty());
}

#[test]
fn wipe_all_leaves_only_the_fresh_store_files() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let mut store = ClipStore::open(&path, Limits::default()).unwrap();
    for at in 0..50 {
        store.record(&text(&format!("wiped-row-{at}"), at)).unwrap();
    }
    let mut store = store.wipe_all(&path).unwrap();
    assert_eq!(store.opened(), &Opened::Clean);
    assert!(store.history(10).unwrap().is_empty());
    assert_absent_everywhere(dir.path(), "wiped-row-");
    for name in names(dir.path()) {
        assert!(
            ["clipboard.db", "clipboard.db-wal", "clipboard.db-shm"].contains(&name.as_str()),
            "wipe_all must not leave {name}"
        );
    }
    store.record(&text("fresh", 1)).unwrap();
    drop(store);
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    assert_eq!(store.history(10).unwrap().len(), 1);
}

#[test]
fn a_failed_rename_rolls_back_the_files_already_moved() {
    let dir = TempDir::new().unwrap();
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    fs::write(&first, b"one").unwrap();
    fs::write(&second, b"two").unwrap();
    let renames = [
        Rename {
            from: first.clone(),
            to: dir.path().join("first-moved"),
        },
        Rename {
            from: second.clone(),
            to: dir.path().join("missing").join("second-moved"),
        },
    ];
    assert!(rename_all(&renames).is_err());
    assert_eq!(fs::read(&first).unwrap(), b"one");
    assert_eq!(fs::read(&second).unwrap(), b"two");
    assert_eq!(names(dir.path()), ["first", "second"]);
}

#[cfg(unix)]
#[test]
fn a_read_only_folder_keeps_the_damaged_file_in_place() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let original = noise(2 * PAGE);
    fs::write(&path, &original).unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
    let probe = fs::write(dir.path().join("probe"), b"");
    let result = ClipStore::open(&path, Limits::default());
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
    if probe.is_ok() {
        return;
    }
    assert!(result.is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(names(dir.path()), ["clipboard.db"]);
}

#[test]
fn wipe_all_with_a_wrong_path_removes_nothing() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let other = dir.path().join("other.db");
    fs::write(&other, b"not ours").unwrap();
    let mut store = ClipStore::open(&path, Limits::default()).unwrap();
    store.record(&text("kept", 1)).unwrap();
    let result = store.wipe_all(&other);
    let error = result.err().unwrap();
    assert!(matches!(error, ClipError::WrongPath));
    assert!(!error.to_string().contains("other.db"));
    assert_eq!(fs::read(&other).unwrap(), b"not ours");
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    assert_eq!(store.history(10).unwrap().len(), 1);
}

#[test]
fn wipe_all_accepts_another_spelling_of_the_open_path() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let store = ClipStore::open(&path, Limits::default()).unwrap();
    let spelled = dir.path().join(".").join("clipboard.db");
    assert!(store.wipe_all(&spelled).is_ok());
}

fn sqlite_failure(code: i32) -> ClipError {
    ClipError::Sqlite(rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(code),
        None,
    ))
}

#[test]
fn only_a_corrupt_or_not_a_database_error_moves_a_file_aside() {
    use rusqlite::ffi;
    let moves = [
        (sqlite_failure(ffi::SQLITE_CORRUPT), true),
        (sqlite_failure(ffi::SQLITE_NOTADB), true),
        (ClipError::Corrupt, true),
        (sqlite_failure(ffi::SQLITE_BUSY), false),
        (sqlite_failure(ffi::SQLITE_LOCKED), false),
        (sqlite_failure(ffi::SQLITE_READONLY), false),
        (sqlite_failure(ffi::SQLITE_IOERR), false),
        (sqlite_failure(ffi::SQLITE_CANTOPEN), false),
        (ClipError::Io(std::io::Error::other("disk")), false),
    ];
    for (error, expected) in moves {
        assert_eq!(error.is_corruption(), expected, "{error:?}");
    }
}

#[test]
fn a_newer_version_wal_database_is_refused_with_its_files_untouched() {
    let dir = TempDir::new().unwrap();
    let path = db_path(&dir);
    let conn = Connection::open(&path).unwrap();
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
        .unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.execute_batch("CREATE TABLE future (x); INSERT INTO future VALUES ('keep');")
        .unwrap();
    conn.pragma_update(None, "user_version", 99).unwrap();
    drop(conn);
    let wal_path = with_suffix(&path, "-wal");
    let (main, wal) = (fs::read(&path).unwrap(), fs::read(&wal_path).unwrap());
    assert!(!wal.is_empty());
    let result = ClipStore::open(&path, Limits::default());
    assert!(matches!(result, Err(ClipError::TooNew { found: 99, .. })));
    assert_eq!(fs::read(&path).unwrap(), main);
    assert_eq!(fs::read(&wal_path).unwrap(), wal);
}

#[test]
fn the_checked_rename_moves_into_a_free_name_and_leaves_no_placeholder() {
    let dir = TempDir::new().unwrap();
    let from = dir.path().join("from");
    let to = dir.path().join("to");
    fs::write(&from, b"data").unwrap();
    rename_if_free(&from, &to).unwrap();
    assert_eq!(fs::read(&to).unwrap(), b"data");
    assert_eq!(names(dir.path()), ["to"]);
}

#[test]
fn the_checked_rename_never_replaces_a_file_and_keeps_both() {
    let dir = TempDir::new().unwrap();
    let from = dir.path().join("from");
    let to = dir.path().join("to");
    fs::write(&from, b"data").unwrap();
    fs::write(&to, b"other").unwrap();
    let error = rename_if_free(&from, &to).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&from).unwrap(), b"data");
    assert_eq!(fs::read(&to).unwrap(), b"other");
}

#[cfg(unix)]
#[test]
fn the_checked_rename_never_replaces_a_dangling_symlink() {
    let dir = TempDir::new().unwrap();
    let from = dir.path().join("from");
    let to = dir.path().join("to");
    fs::write(&from, b"data").unwrap();
    std::os::unix::fs::symlink(dir.path().join("nowhere"), &to).unwrap();
    assert!(rename_if_free(&from, &to).is_err());
    assert_eq!(fs::read(&from).unwrap(), b"data");
    assert!(fs::symlink_metadata(&to).unwrap().file_type().is_symlink());
}
