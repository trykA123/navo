use super::*;

#[test]
fn a_v5_database_has_no_retention_data_to_reset_during_salvage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let conn = Connection::open(&path).unwrap();
    for sql in &MIGRATIONS[..5] {
        conn.execute_batch(sql).unwrap();
    }
    conn.pragma_update(None, "user_version", 5).unwrap();
    drop(conn);
    let fresh = Db::open_in_memory().unwrap();
    let report = crate::salvage_database(&path, &fresh).unwrap();
    assert!(
        report
            .tables
            .iter()
            .any(|table| table.name == "retained_entry"
                && table.outcome == crate::SalvageOutcome::Copied { rows: 0 })
    );
    assert!(report.retained.is_empty());
}

#[test]
fn migration_from_v5_preserves_existing_job_and_user_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v5.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    for sql in &MIGRATIONS[..5] {
        conn.execute_batch(sql).unwrap();
    }
    conn.pragma_update(None, "user_version", 5).unwrap();
    conn.execute("INSERT INTO tags VALUES ('/sample', 'keep')", [])
        .unwrap();
    drop(conn);
    let (db, report) = Db::open(&path, false).unwrap();
    assert_eq!((report.from_version, report.to_version), (5, Db::VERSION));
    assert_eq!(
        db.connection()
            .query_row("SELECT tag FROM tags", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    assert!(
        crate::RetentionCatalog::new(db)
            .unwrap()
            .list_after(0, 10)
            .unwrap()
            .is_empty()
    );
}
