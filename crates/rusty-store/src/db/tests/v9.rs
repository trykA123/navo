use super::*;

fn sql_now(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT CAST(unixepoch('subsec') * 1000 AS INTEGER)",
        [],
        |row| row.get(0),
    )
    .unwrap()
}

fn terminal_times(conn: &Connection) -> Vec<(String, Option<i64>)> {
    let mut statement = conn
        .prepare("SELECT state, terminal_ms FROM retained_entry ORDER BY id")
        .unwrap();
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

#[test]
fn migration_from_v8_backfills_terminal_rows_and_leaves_live_rows_empty() {
    let dir = tmp();
    let path = db_path(&dir);
    build_at_version(&path, 8);
    let conn = Connection::open(&path).unwrap();
    for state in ["purged", "ready", "restored", "aborted", "purging"] {
        conn.execute(
            "INSERT INTO retained_entry (state, record) VALUES (?1, X'ff')",
            [state],
        )
        .unwrap();
    }
    let before = sql_now(&conn);
    drop(conn);
    let (db, report) = Db::open(&path, false).unwrap();
    assert_eq!((report.from_version, report.to_version), (8, 9));
    let after = sql_now(db.connection());
    for (state, stamped) in terminal_times(db.connection()) {
        let terminal = matches!(state.as_str(), "purged" | "restored" | "aborted");
        assert_eq!(stamped.is_some(), terminal, "{state}");
        assert!(stamped.is_none_or(|stamped| (before..=after).contains(&stamped)));
    }
    let index: i64 = db
        .connection()
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'index' AND name = 'retained_entry_prunable'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(index, 1);
}
