use super::*;

#[test]
fn migration_from_v6_preserves_rows_and_adds_indexed_cleanup_projections() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("v6.db");
    let conn = Connection::open(&path).unwrap();
    for sql in &MIGRATIONS[..6] {
        conn.execute_batch(sql).unwrap();
    }
    conn.pragma_update(None, "user_version", 6).unwrap();
    conn.execute("INSERT INTO tags VALUES ('/sample', 'keep')", [])
        .unwrap();
    conn.execute(
        "INSERT INTO retained_entry (state, record) VALUES ('aborted', X'ff')",
        [],
    )
    .unwrap();
    drop(conn);
    let (db, report) = Db::open(&path, false).unwrap();
    assert_eq!((report.from_version, report.to_version), (6, Db::VERSION));
    assert_eq!(
        db.connection()
            .query_row("SELECT count(*) FROM tags", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    let projected: (Option<String>, i64) = db
        .connection()
        .query_row(
            "SELECT container_path, cleanup_pending FROM retained_entry",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(projected, (None, 1));
    let indexes: i64 = db.connection().query_row("SELECT count(*) FROM sqlite_schema WHERE type = 'index' AND name IN ('retained_entry_cleanup', 'retained_entry_live_container')", [], |row| row.get(0)).unwrap();
    assert_eq!(indexes, 2);
}

#[test]
fn opening_the_current_schema_does_not_wait_for_an_unrelated_writer() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("current.db");
    let (db, _) = Db::open(&path, false).unwrap();
    db.connection()
        .execute_batch("BEGIN IMMEDIATE; INSERT INTO tags VALUES ('/held', 'pending');")
        .unwrap();
    let (_, report) = Db::open(&path, false).unwrap();
    assert_eq!(report.from_version, Db::VERSION);
    db.connection().execute_batch("ROLLBACK;").unwrap();
}
