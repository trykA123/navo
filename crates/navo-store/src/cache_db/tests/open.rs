use std::cell::RefCell;

use super::*;
use crate::cache_db::migrations;

const PAGE: usize = 4096;

fn garbage() -> Vec<u8> {
    (0..8192_u32).map(|i| (i * 31 + 7) as u8).collect()
}

fn write_v0(path: &Path) {
    Connection::open(path)
        .unwrap()
        .execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE blob (
                 tbl TEXT, key BLOB, value BLOB, size INTEGER, accessed INTEGER,
                 PRIMARY KEY(tbl, key)
             );
             CREATE INDEX blob_age ON blob (tbl, accessed, key, size);
             INSERT INTO blob (tbl, key, value, size, accessed) VALUES
                 ('thumbs', x'61', x'6f6e65', 4, 1),
                 ('thumbs', x'62', x'74776f', 4, 2),
                 ('sizes', x'78', x'7979', 3, 3);",
        )
        .unwrap();
}

thread_local! {
    static SECOND: RefCell<Option<Connection>> = const { RefCell::new(None) };
    static BLOCKER: RefCell<Option<Connection>> = const { RefCell::new(None) };
    static SECOND_RESULT: RefCell<Option<Result<CacheDb, String>>> = const { RefCell::new(None) };
}

fn migrate_second(_: i32) -> bool {
    BLOCKER.with(|slot| {
        slot.borrow_mut()
            .take()
            .expect("the blocker connection")
            .execute_batch("ROLLBACK")
            .expect("release the write lock");
    });
    let second = SECOND.with(|slot| slot.borrow_mut().take().expect("the second connection"));
    let opened = CacheDb::from_connection(second).map_err(|error| error.to_string());
    SECOND_RESULT.with(|slot| *slot.borrow_mut() = Some(opened));
    true
}

fn open(dir: &TempDir) -> (CacheDb, CacheOpen) {
    CacheDb::open(&cache_path(dir)).unwrap()
}

fn open_report(dir: &TempDir) -> CacheOpen {
    open(dir).1
}

fn write_sqlite_with_damaged_page(path: &Path) {
    Connection::open(path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE filler (id INTEGER PRIMARY KEY, text TEXT);
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 2000)
             INSERT INTO filler (text) SELECT printf('row-%05d-with-some-padding-text', i) FROM n;",
        )
        .unwrap();
    let mut bytes = fs::read(path).unwrap();
    let pages = bytes.len() / PAGE;
    assert!(pages >= 8, "only {pages} pages");
    let start = pages / 2 * PAGE;
    for (i, byte) in bytes[start..start + PAGE].iter_mut().enumerate() {
        *byte = (i * 31 + 7) as u8;
    }
    fs::write(path, bytes).unwrap();
}

#[test]
fn missing_file_is_created() {
    let dir = tmp();
    assert_eq!(open_report(&dir), CacheOpen::Created);
}

#[test]
fn missing_file_exists_after_open() {
    let dir = tmp();
    let _cache = open(&dir);
    assert!(cache_path(&dir).is_file());
}

#[test]
fn existing_file_is_opened() {
    let dir = tmp();
    drop(open(&dir));
    assert_eq!(open_report(&dir), CacheOpen::Opened);
}

#[test]
fn garbage_file_is_recreated() {
    let dir = tmp();
    fs::write(cache_path(&dir), garbage()).unwrap();
    let want = CacheOpen::Recreated {
        moved_to: corrupt_path(&dir, 1),
    };
    assert_eq!(open_report(&dir), want);
}

#[test]
fn garbage_file_is_moved_with_its_bytes() {
    let dir = tmp();
    fs::write(cache_path(&dir), garbage()).unwrap();
    let _cache = open(&dir);
    assert_eq!(fs::read(corrupt_path(&dir, 1)).unwrap(), garbage());
}

#[test]
fn recreated_file_opens_cleanly_next_time() {
    let dir = tmp();
    fs::write(cache_path(&dir), garbage()).unwrap();
    drop(open(&dir));
    assert_eq!(open_report(&dir), CacheOpen::Opened);
}

#[test]
fn recreated_cache_accepts_puts() {
    let dir = tmp();
    fs::write(cache_path(&dir), garbage()).unwrap();
    let (mut cache, _) = open(&dir);
    cache.put("thumbs", b"a", b"one").unwrap();
    assert_eq!(cache.get("thumbs", b"a").unwrap(), Some(b"one".to_vec()));
}

#[test]
fn second_corruption_moves_to_corrupt_2() {
    let dir = tmp();
    fs::write(cache_path(&dir), garbage()).unwrap();
    drop(open(&dir));
    fs::write(cache_path(&dir), garbage()).unwrap();
    let want = CacheOpen::Recreated {
        moved_to: corrupt_path(&dir, 2),
    };
    assert_eq!(open_report(&dir), want);
}

#[test]
fn second_corruption_keeps_the_first_moved_file() {
    let dir = tmp();
    fs::write(cache_path(&dir), garbage()).unwrap();
    drop(open(&dir));
    fs::write(cache_path(&dir), b"second").unwrap();
    drop(open(&dir));
    assert_eq!(fs::read(corrupt_path(&dir, 1)).unwrap(), garbage());
}

#[test]
fn a_damaged_page_is_not_scanned_for_at_open() {
    let dir = tmp();
    write_sqlite_with_damaged_page(&cache_path(&dir));
    assert_eq!(open_report(&dir), CacheOpen::Opened);
}

#[test]
fn data_survives_close_and_reopen() {
    let dir = tmp();
    let (mut cache, _) = open(&dir);
    cache.put("thumbs", b"a", b"one").unwrap();
    drop(cache);
    let (cache, _) = open(&dir);
    assert_eq!(cache.get("thumbs", b"a").unwrap(), Some(b"one".to_vec()));
}

#[test]
fn used_bytes_survive_close_and_reopen() {
    let dir = tmp();
    let (mut cache, _) = open(&dir);
    cache.put("thumbs", b"abc", b"12345").unwrap();
    drop(cache);
    let (cache, _) = open(&dir);
    assert_eq!(cache.used_bytes("thumbs").unwrap(), 8);
}

#[test]
fn two_connections_migrate_a_version_zero_file_once() {
    let dir = tmp();
    let path = cache_path(&dir);
    write_v0(&path);
    let blocker = Connection::open(&path).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    BLOCKER.with(|slot| *slot.borrow_mut() = Some(blocker));
    SECOND.with(|slot| *slot.borrow_mut() = Some(Connection::open(&path).unwrap()));

    let first = Connection::open(&path).unwrap();
    first.busy_handler(Some(migrate_second)).unwrap();
    let first = CacheDb::from_connection(first).map_err(|error| error.to_string());
    let second = SECOND_RESULT.with(|slot| slot.borrow_mut().take()).unwrap();

    for opened in [&first, &second] {
        assert!(
            opened.is_ok(),
            "{}",
            opened.as_ref().err().map_or("", String::as_str)
        );
    }
    let first = first.expect("the first open succeeds");
    let second = second.expect("the second open succeeds");
    for cache in [&first, &second] {
        for table in ["thumbs", "sizes"] {
            let sum: i64 = cache
                .conn
                .query_row(
                    "SELECT COALESCE(SUM(size), 0) FROM blob WHERE tbl = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(
                cache.used_bytes(table).unwrap(),
                u64::try_from(sum).unwrap(),
                "stored total for {table}"
            );
        }
        let version: u32 = cache
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, migrations::VERSION);
    }
}

#[test]
fn a_newer_cache_version_is_refused() {
    let dir = tmp();
    let path = cache_path(&dir);
    write_v0(&path);
    Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", migrations::VERSION + 1)
        .unwrap();
    let error = CacheDb::open(&path)
        .err()
        .expect("a newer cache is refused");
    assert!(
        matches!(
            error,
            CacheError::TooNew { found, supported }
                if found == migrations::VERSION + 1 && supported == migrations::VERSION
        ),
        "{error:?}"
    );
    assert!(
        !corrupt_path(&dir, 1).exists(),
        "the cache is left in place"
    );
}
