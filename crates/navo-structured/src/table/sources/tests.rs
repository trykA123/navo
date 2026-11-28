use std::fs;
use std::path::Path;

use rusqlite::Connection;
use tempfile::TempDir;

use super::{
    BLOB_PREVIEW_BYTES, SheetInfo, SheetKind, SourceError, SqliteTableSource, quote_identifier,
};

const HOSTILE_TABLE: &str = "foo\"; DROP TABLE users; --";

fn create_fixture_db(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("fixture.sqlite");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY,
            name TEXT,
            score REAL,
            payload BLOB
        );
        CREATE TABLE orders (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL,
            amount REAL NOT NULL
        );
        CREATE VIEW active_users AS
            SELECT id, name FROM users WHERE name IS NOT NULL ORDER BY id;",
    )
    .unwrap();

    let quoted_hostile = format!("\"{}\"", HOSTILE_TABLE.replace('"', "\"\""));
    conn.execute_batch(&format!(
        "CREATE TABLE {quoted_hostile} (k TEXT, v INTEGER);
         INSERT INTO {quoted_hostile} (k, v) VALUES ('safe', 42);"
    ))
    .unwrap();

    let small_blob = vec![0xDE_u8, 0xAD, 0xBE, 0xEF];
    let large_blob = vec![0xAB_u8; 256 * 1024];

    conn.execute(
        "INSERT INTO users (id, name, score, payload) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![1_i64, "Alice", 98.5_f64, small_blob],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO users (id, name, score, payload) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            2_i64,
            Option::<String>::None,
            Option::<f64>::None,
            large_blob
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO users (id, name, score, payload) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![3_i64, "Bob", 75.0_f64, Vec::<u8>::new()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO orders (id, user_id, amount) VALUES (10, 1, 19.99), (11, 3, 4.50)",
        [],
    )
    .unwrap();
    drop(conn);
    path
}

#[test]
fn quote_identifier_escapes_embedded_double_quotes() {
    assert_eq!(quote_identifier("users"), "\"users\"");
    assert_eq!(
        quote_identifier(HOSTILE_TABLE),
        "\"foo\"\"; DROP TABLE users; --\""
    );
}

#[test]
fn opens_tables_views_hostile_names_mixed_types_and_pages_rows() {
    let dir = TempDir::new().unwrap();
    let db_path = create_fixture_db(dir.path());
    let before = fs::read(&db_path).unwrap();

    let source = SqliteTableSource::open(&db_path).unwrap();
    assert_eq!(
        source.sheets(),
        &[
            SheetInfo {
                name: "active_users".to_owned(),
                kind: SheetKind::View,
            },
            SheetInfo {
                name: HOSTILE_TABLE.to_owned(),
                kind: SheetKind::Table,
            },
            SheetInfo {
                name: "orders".to_owned(),
                kind: SheetKind::Table,
            },
            SheetInfo {
                name: "users".to_owned(),
                kind: SheetKind::Table,
            },
        ]
    );

    assert_eq!(
        source.sheet_columns("users").unwrap(),
        vec!["id", "name", "score", "payload"]
    );
    assert_eq!(source.sheet_row_count("users").unwrap(), 3);

    let page1 = source.read_page("users", 0, 2).unwrap();
    assert_eq!(page1.columns, vec!["id", "name", "score", "payload"]);
    assert_eq!(page1.total_rows, 3);
    assert_eq!(page1.offset, 0);
    assert_eq!(page1.rows.len(), 2);
    assert_eq!(page1.rows[0], vec!["1", "Alice", "98.5", "0xdeadbeef"]);
    assert_eq!(page1.rows[1][0], "2");
    assert_eq!(page1.rows[1][1], "NULL");
    assert_eq!(page1.rows[1][2], "NULL");
    let blob_cell = &page1.rows[1][3];
    assert!(
        blob_cell.starts_with("0x"),
        "unexpected blob cell: {blob_cell}"
    );
    assert!(
        blob_cell.contains("262144 bytes"),
        "expected byte length in truncated blob preview: {blob_cell}"
    );
    assert!(
        blob_cell.len() < BLOB_PREVIEW_BYTES * 2 + 32,
        "blob preview must stay bounded: len={}",
        blob_cell.len()
    );

    let page2 = source.read_page("users", 2, 2).unwrap();
    assert_eq!(page2.rows.len(), 1);
    assert_eq!(page2.offset, 2);
    assert_eq!(page2.rows[0], vec!["3", "Bob", "75", "0x"]);

    let view_page = source.read_page("active_users", 0, 10).unwrap();
    assert_eq!(view_page.columns, vec!["id", "name"]);
    assert_eq!(view_page.total_rows, 2);
    assert_eq!(view_page.rows, vec![vec!["1", "Alice"], vec!["3", "Bob"]]);

    let hostile_page = source.read_page(HOSTILE_TABLE, 0, 10).unwrap();
    assert_eq!(hostile_page.columns, vec!["k", "v"]);
    assert_eq!(hostile_page.rows, vec![vec!["safe", "42"]]);
    assert_eq!(source.sheet_row_count("users").unwrap(), 3);

    let err = source.read_page("does_not_exist", 0, 10).unwrap_err();
    assert!(matches!(err, SourceError::SheetNotFound { .. }));

    drop(source);
    let after = fs::read(&db_path).unwrap();
    assert_eq!(
        before, after,
        "read-only open/read must not mutate db bytes"
    );
}

#[test]
fn write_statements_fail_and_leave_database_bytes_identical() {
    let dir = TempDir::new().unwrap();
    let db_path = create_fixture_db(dir.path());
    let before = fs::read(&db_path).unwrap();

    let source = SqliteTableSource::open(&db_path).unwrap();
    let write_statements = [
        "DROP TABLE users",
        "INSERT INTO users (id, name, score, payload) VALUES (99, 'Mallory', 0.0, NULL)",
        "UPDATE users SET name = 'Mallory' WHERE id = 1",
        "DELETE FROM users WHERE id = 1",
        "CREATE TABLE evil (id INTEGER)",
        "ATTACH DATABASE ':memory:' AS extra",
        "PRAGMA user_version = 99",
    ];

    for sql in write_statements {
        let result = source.query_read_only(sql, 100);
        assert!(
            matches!(result, Err(SourceError::ReadOnly { .. })),
            "expected ReadOnly error for `{sql}`, got {result:?}"
        );
    }

    let multi = source.query_read_only("SELECT 1; DROP TABLE users;", 100);
    assert!(multi.is_err(), "multi-statement SQL must be rejected");

    let select = source
        .query_read_only("SELECT id, name FROM users ORDER BY id", 2)
        .unwrap();
    assert_eq!(select.columns, vec!["id", "name"]);
    assert_eq!(select.rows, vec![vec!["1", "Alice"], vec!["2", "NULL"]]);
    assert!(select.truncated);

    drop(source);
    let after = fs::read(&db_path).unwrap();
    assert_eq!(
        before, after,
        "failed write statements must leave database bytes 100% identical"
    );
    assert!(!dir.path().join("fixture.sqlite-wal").exists());
    assert!(!dir.path().join("fixture.sqlite-shm").exists());
}

#[test]
fn rejects_missing_empty_non_sqlite_and_corrupt_files_without_panicking() {
    let dir = TempDir::new().unwrap();

    let missing = dir.path().join("missing.sqlite");
    assert!(matches!(
        SqliteTableSource::open(&missing),
        Err(SourceError::Io { .. })
    ));

    let empty = dir.path().join("empty.sqlite");
    fs::write(&empty, b"").unwrap();
    assert!(matches!(
        SqliteTableSource::open(&empty),
        Err(SourceError::NotSqlite { .. })
    ));

    let plain = dir.path().join("plain.txt");
    fs::write(&plain, b"hello, this is definitely not a sqlite database").unwrap();
    assert!(matches!(
        SqliteTableSource::open(&plain),
        Err(SourceError::NotSqlite { .. })
    ));

    let corrupt = dir.path().join("corrupt.sqlite");
    let mut garbage = b"SQLite format 3\0".to_vec();
    garbage.extend_from_slice(&[0xFF_u8; 4096]);
    fs::write(&corrupt, &garbage).unwrap();
    assert!(matches!(
        SqliteTableSource::open(&corrupt),
        Err(SourceError::Corrupt { .. } | SourceError::Sqlite(_))
    ));
}

#[test]
fn recursive_cte_view_and_query_abort_promptly_with_limit_error() {
    use std::sync::mpsc;
    use std::time::Duration;

    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("loop.sqlite");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE VIEW loop_v AS \
         WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) \
         SELECT x FROM c;",
    )
    .unwrap();
    drop(conn);

    let (tx, rx) = mpsc::channel();
    let db_path_clone = db_path.clone();
    let handle = std::thread::spawn(move || {
        let source = SqliteTableSource::open(&db_path_clone).unwrap();
        let page_res = source.read_page("loop_v", 0, 10);
        let query_res = source.query_read_only(
            "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) \
             SELECT count(*) FROM c",
            10,
        );
        let _ = tx.send((page_res, query_res));
    });

    let (page_res, query_res) = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("recursive CTE view/query hung instead of aborting promptly");
    handle.join().unwrap();

    assert!(
        matches!(page_res, Err(SourceError::QueryLimitExceeded { .. })),
        "expected QueryLimitExceeded on infinite recursive view, got {page_res:?}"
    );
    assert!(
        matches!(query_res, Err(SourceError::QueryLimitExceeded { .. })),
        "expected QueryLimitExceeded on infinite recursive query, got {query_res:?}"
    );
}

#[test]
fn pragma_query_only_off_is_rejected_and_guard_stays_on() {
    let dir = TempDir::new().unwrap();
    let db_path = create_fixture_db(dir.path());
    let mut source = SqliteTableSource::open(&db_path).unwrap();

    let res = source.query_read_only("PRAGMA query_only = OFF", 10);
    assert!(
        matches!(res, Err(SourceError::ReadOnly { .. })),
        "expected ReadOnly error for `PRAGMA query_only = OFF`, got {res:?}"
    );
    assert!(
        source.is_query_only_enabled(),
        "PRAGMA query_only must remain ON after rejected PRAGMA statement"
    );
}

#[cfg(unix)]
#[test]
fn opens_wal_database_in_read_only_directory_without_creating_shm() {
    use std::os::unix::fs::PermissionsExt;

    struct RestoreDirMode<'a>(&'a Path);
    impl Drop for RestoreDirMode<'_> {
        fn drop(&mut self) {
            let _ = fs::set_permissions(self.0, fs::Permissions::from_mode(0o755));
        }
    }

    let dir = TempDir::new().unwrap();
    let ro_dir = dir.path().join("ro");
    fs::create_dir(&ro_dir).unwrap();
    let db_path = ro_dir.join("wal.sqlite");

    let conn = Connection::open(&db_path).unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode = WAL;", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode.to_ascii_lowercase(), "wal");
    conn.execute_batch(
        "CREATE TABLE items (id INTEGER PRIMARY KEY, label TEXT);
         INSERT INTO items (id, label) VALUES (1, 'alpha'), (2, 'beta');
         PRAGMA wal_checkpoint(TRUNCATE);",
    )
    .unwrap();
    drop(conn);

    fs::set_permissions(&ro_dir, fs::Permissions::from_mode(0o555)).unwrap();
    let _restore = RestoreDirMode(&ro_dir);

    let source = SqliteTableSource::open(&db_path).unwrap();
    let page = source.read_page("items", 0, 10).unwrap();
    assert_eq!(page.total_rows, 2);
    assert_eq!(page.rows, vec![vec!["1", "alpha"], vec!["2", "beta"]]);
    assert!(
        !ro_dir.join("wal.sqlite-shm").exists(),
        "opening WAL db in read-only dir must not create -shm"
    );
}

#[test]
fn sqlite_immutable_uri_handles_unix_drive_unc_and_verbatim_paths() {
    use super::sqlite::sqlite_immutable_uri;

    assert_eq!(
        sqlite_immutable_uri(Path::new("/tmp/a b?c#d%20.db")).as_deref(),
        Some("file:///tmp/a%20b%3Fc%23d%2520.db?mode=ro&immutable=1")
    );
    assert_eq!(
        sqlite_immutable_uri(Path::new(r"C:\data\test#1.db")).as_deref(),
        Some("file:///C:/data/test%231.db?mode=ro&immutable=1")
    );
    assert_eq!(
        sqlite_immutable_uri(Path::new(r"\\server\share\sub\foo.db")).as_deref(),
        Some("file://server/share/sub/foo.db?mode=ro&immutable=1")
    );
    assert_eq!(
        sqlite_immutable_uri(Path::new(r"\\?\C:\data\foo.db")).as_deref(),
        Some("file:///C:/data/foo.db?mode=ro&immutable=1")
    );
    assert_eq!(
        sqlite_immutable_uri(Path::new(r"\\?\UNC\server\share\foo.db")).as_deref(),
        Some("file://server/share/foo.db?mode=ro&immutable=1")
    );
}
