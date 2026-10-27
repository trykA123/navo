use rusqlite::types::Value;

use super::*;

fn v4_file(dir: &TempDir) -> PathBuf {
    let path = db_path(dir);
    build_from_sql(&path, V3_SQL);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(MIGRATIONS[3]).unwrap();
    conn.pragma_update(None, "user_version", 4).unwrap();
    conn.execute(
        "INSERT INTO command_use (name, score, last_at) VALUES ('view.toggle_hidden', 2.0, 5)",
        [],
    )
    .unwrap();
    path
}

fn block_v5(path: &Path) {
    Connection::open(path)
        .unwrap()
        .execute_batch("CREATE TABLE rename_preset (blocker INTEGER);")
        .unwrap();
}

fn every_row(conn: &Connection) -> Vec<(String, Vec<Vec<Value>>)> {
    table_names(conn)
        .into_iter()
        .map(|table| {
            let mut stmt = conn
                .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
                .unwrap();
            let width = stmt.column_count();
            let rows = stmt
                .query_map([], |row| {
                    (0..width).map(|i| row.get::<_, Value>(i)).collect()
                })
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            (table, rows)
        })
        .collect()
}

fn user_version(path: &Path) -> i64 {
    Connection::open(path)
        .unwrap()
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

#[test]
fn v4_reports_its_starting_version() {
    let dir = tmp();
    let (_, report) = Db::open(&v4_file(&dir), false).unwrap();
    assert_eq!((report.from_version, report.to_version), (4, Db::VERSION));
}

#[test]
fn v4_rows_survive_v5_value_for_value() {
    let dir = tmp();
    let path = v4_file(&dir);
    let want = every_row(&Connection::open(&path).unwrap());
    let (db, _) = Db::open(&path, false).unwrap();
    let mut got = every_row(db.connection());
    got.retain(|(table, _)| {
        !matches!(
            table.as_str(),
            "collection_items"
                | "collections"
                | "rename_preset"
                | "retained_entry"
                | "sqlite_sequence"
        )
    });
    assert_eq!(got, want);
}

#[test]
fn v4_gets_an_empty_preset_table_with_its_columns() {
    let dir = tmp();
    let (db, _) = Db::open(&v4_file(&dir), false).unwrap();
    let columns: Vec<String> = db
        .connection()
        .prepare("SELECT name FROM pragma_table_info('rename_preset') ORDER BY cid")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(columns, ["name", "rules", "saved_at"]);
    assert_eq!(db.presets().unwrap(), RenamePresets::default());
}

#[test]
fn a_failure_inside_v5_is_reported() {
    let dir = tmp();
    let path = v4_file(&dir);
    block_v5(&path);
    assert!(Db::open(&path, false).is_err());
}

#[test]
fn a_failure_inside_v5_leaves_version_four() {
    let dir = tmp();
    let path = v4_file(&dir);
    block_v5(&path);
    let _ = Db::open(&path, false);
    assert_eq!(user_version(&path), 4);
}

#[test]
fn a_failure_inside_v5_keeps_every_row_and_table() {
    let dir = tmp();
    let path = v4_file(&dir);
    block_v5(&path);
    let want = every_row(&Connection::open(&path).unwrap());
    let _ = Db::open(&path, false);
    assert_eq!(every_row(&Connection::open(&path).unwrap()), want);
}

#[test]
fn a_newer_file_than_v5_is_refused_and_left_unchanged() {
    let dir = tmp();
    let path = v4_file(&dir);
    drop(Db::open(&path, false).unwrap());
    set_user_version(&path, Db::VERSION + 1);
    let before = fs::read(&path).unwrap();
    let err = Db::open(&path, false).err().unwrap();
    assert!(
        matches!(
            err,
            DbError::TooNew { found, supported } if found == Db::VERSION + 1 && supported == Db::VERSION
        ),
        "{err:?}"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
}
