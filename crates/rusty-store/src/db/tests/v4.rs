use rusqlite::types::Value;

use super::*;

type Snapshot = Vec<(String, Vec<String>, Vec<Vec<Value>>)>;

fn column_names(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")
        .unwrap();
    stmt.query_map([table], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn rows(conn: &Connection, table: &str, columns: &[String]) -> Vec<Vec<Value>> {
    let sql = format!("SELECT {} FROM {table} ORDER BY rowid", columns.join(", "));
    let mut stmt = conn.prepare(&sql).unwrap();
    stmt.query_map([], |row| {
        (0..columns.len()).map(|i| row.get::<_, Value>(i)).collect()
    })
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}

fn snapshot(conn: &Connection) -> Snapshot {
    table_names(conn)
        .into_iter()
        .map(|table| {
            let columns = column_names(conn, &table);
            let rows = rows(conn, &table, &columns);
            (table, columns, rows)
        })
        .collect()
}

fn snapshot_like(conn: &Connection, want: &Snapshot) -> Snapshot {
    want.iter()
        .map(|(table, columns, _)| (table.clone(), columns.clone(), rows(conn, table, columns)))
        .collect()
}

fn schema(conn: &Connection) -> Vec<(String, Option<String>)> {
    let mut stmt = conn
        .prepare("SELECT name, sql FROM sqlite_master WHERE name != 'frecency' ORDER BY name")
        .unwrap();
    stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn pragma_i64(conn: &Connection, name: &str) -> i64 {
    conn.pragma_query_value(None, name, |row| row.get(0))
        .unwrap()
}

fn v3_file(dir: &TempDir) -> PathBuf {
    let path = db_path(dir);
    build_from_sql(&path, V3_SQL);
    path
}

fn block_v4(path: &Path) {
    Connection::open(path)
        .unwrap()
        .execute_batch("CREATE TABLE command_use (blocker INTEGER);")
        .unwrap();
}

fn v4_file_with_rows(dir: &TempDir) -> PathBuf {
    let path = v3_file(dir);
    let (db, _) = Db::open(&path, false).unwrap();
    db.record_visit("file:///home/admin/v4", false, 1_760_800_000_000)
        .unwrap();
    db.record_command("view.toggle_hidden", 1_760_800_000_000)
        .unwrap();
    path
}

#[test]
fn v3_fixture_has_rows_in_every_table() {
    let dir = tmp();
    let conn = Connection::open(v3_file(&dir)).unwrap();
    let empty: Vec<String> = snapshot(&conn)
        .into_iter()
        .filter(|(_, _, rows)| rows.is_empty())
        .map(|(table, _, _)| table)
        .collect();
    assert!(empty.is_empty(), "empty tables: {empty:?}");
}

#[test]
fn v3_fixture_reports_its_starting_version() {
    let dir = tmp();
    let (_, report) = Db::open(&v3_file(&dir), false).unwrap();
    assert_eq!((report.from_version, report.to_version), (3, Db::VERSION));
}

#[test]
fn v3_fixture_gets_every_table() {
    let dir = tmp();
    let (db, _) = Db::open(&v3_file(&dir), false).unwrap();
    assert_eq!(table_names(db.connection()), TABLES);
}

#[test]
fn v3_rows_survive_v4_value_for_value() {
    let dir = tmp();
    let path = v3_file(&dir);
    let want = snapshot(&Connection::open(&path).unwrap());
    let (db, _) = Db::open(&path, false).unwrap();
    assert_eq!(snapshot_like(db.connection(), &want), want);
}

#[test]
fn v3_schema_outside_frecency_is_kept() {
    let dir = tmp();
    let path = v3_file(&dir);
    let want = schema(&Connection::open(&path).unwrap());
    let (db, _) = Db::open(&path, false).unwrap();
    let mut got = schema(db.connection());
    got.retain(|(name, _)| !name.contains("command_use") && !name.contains("rename_preset"));
    assert_eq!(got, want);
}

#[test]
fn v3_frecency_rows_count_as_folders() {
    let dir = tmp();
    let (db, _) = Db::open(&v3_file(&dir), false).unwrap();
    let files: i64 = db
        .connection()
        .query_row(
            "SELECT count(*) FROM frecency WHERE is_folder != 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(files, 0);
}

#[test]
fn an_empty_file_gets_the_is_folder_column() {
    let dir = tmp();
    let (db, _) = Db::open(&db_path(&dir), false).unwrap();
    assert_eq!(
        column_names(db.connection(), "frecency"),
        ["location", "score", "last_at", "is_folder"]
    );
}

#[test]
fn an_empty_file_gets_the_command_use_table() {
    let dir = tmp();
    let (db, _) = Db::open(&db_path(&dir), false).unwrap();
    assert_eq!(
        column_names(db.connection(), "command_use"),
        ["name", "score", "last_at"]
    );
}

#[test]
fn a_failure_inside_v4_is_reported() {
    let dir = tmp();
    let path = v3_file(&dir);
    block_v4(&path);
    assert!(Db::open(&path, false).is_err());
}

#[test]
fn a_failure_inside_v4_leaves_version_three() {
    let dir = tmp();
    let path = v3_file(&dir);
    block_v4(&path);
    let _ = Db::open(&path, false);
    assert_eq!(
        pragma_i64(&Connection::open(&path).unwrap(), "user_version"),
        3
    );
}

#[test]
fn a_failure_inside_v4_rolls_back_the_new_column() {
    let dir = tmp();
    let path = v3_file(&dir);
    block_v4(&path);
    let _ = Db::open(&path, false);
    assert_eq!(
        column_names(&Connection::open(&path).unwrap(), "frecency"),
        ["location", "score", "last_at"]
    );
}

#[test]
fn a_failure_inside_v4_keeps_every_row() {
    let dir = tmp();
    let path = v3_file(&dir);
    block_v4(&path);
    let want = snapshot(&Connection::open(&path).unwrap());
    let _ = Db::open(&path, false);
    assert_eq!(snapshot(&Connection::open(&path).unwrap()), want);
}

#[test]
fn reopening_a_v4_file_reports_no_migration() {
    let dir = tmp();
    let path = v4_file_with_rows(&dir);
    let (_, report) = Db::open(&path, false).unwrap();
    assert_eq!(
        (report.from_version, report.to_version),
        (Db::VERSION, Db::VERSION)
    );
}

#[test]
fn reopening_a_v4_file_leaves_the_schema_alone() {
    let dir = tmp();
    let path = v4_file_with_rows(&dir);
    let want = pragma_i64(&Connection::open(&path).unwrap(), "schema_version");
    let (db, _) = Db::open(&path, false).unwrap();
    assert_eq!(pragma_i64(db.connection(), "schema_version"), want);
}

#[test]
fn reopening_a_v4_file_keeps_every_row() {
    let dir = tmp();
    let path = v4_file_with_rows(&dir);
    let want = snapshot(&Connection::open(&path).unwrap());
    let (db, _) = Db::open(&path, false).unwrap();
    assert_eq!(snapshot(db.connection()), want);
}

#[test]
fn a_newer_file_with_v4_rows_is_refused_as_too_new() {
    let dir = tmp();
    let path = v4_file_with_rows(&dir);
    set_user_version(&path, Db::VERSION + 1);
    let err = Db::open(&path, false).err().unwrap();
    assert!(
        matches!(
            err,
            DbError::TooNew {
                found: 6,
                supported: 5
            }
        ),
        "{err:?}"
    );
}

#[test]
fn a_newer_file_with_v4_rows_is_left_unchanged() {
    let dir = tmp();
    let path = v4_file_with_rows(&dir);
    set_user_version(&path, Db::VERSION + 1);
    let before = fs::read(&path).unwrap();
    let _ = Db::open(&path, false);
    assert_eq!(fs::read(&path).unwrap(), before);
}
