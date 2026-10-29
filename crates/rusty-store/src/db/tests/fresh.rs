use super::*;

fn open_fresh(dir: &TempDir) -> (Db, OpenReport) {
    Db::open(&db_path(dir), false).unwrap()
}

fn pragma<T: rusqlite::types::FromSql>(db: &Db, name: &str) -> T {
    db.connection()
        .pragma_query_value(None, name, |row| row.get(0))
        .unwrap()
}

#[test]
fn missing_file_is_created() {
    let dir = tmp();
    let _db = open_fresh(&dir);
    assert!(db_path(&dir).is_file());
}

#[test]
fn missing_file_is_created_at_current_version() {
    let dir = tmp();
    let (db, _) = open_fresh(&dir);
    assert_eq!(db.version().unwrap(), Db::VERSION);
}

#[test]
fn missing_file_reports_migration_from_zero_without_check() {
    let dir = tmp();
    let (_, report) = open_fresh(&dir);
    let want = OpenReport {
        from_version: 0,
        to_version: Db::VERSION,
        checked: false,
    };
    assert_eq!(report, want);
}

#[test]
fn missing_file_gets_every_table() {
    let dir = tmp();
    let (db, _) = open_fresh(&dir);
    assert_eq!(table_names(db.connection()), TABLES);
}

#[test]
fn open_sets_journal_mode_wal() {
    let dir = tmp();
    let (db, _) = open_fresh(&dir);
    assert_eq!(pragma::<String>(&db, "journal_mode"), "wal");
}

#[test]
fn open_sets_synchronous_normal() {
    let dir = tmp();
    let (db, _) = open_fresh(&dir);
    assert_eq!(pragma::<i64>(&db, "synchronous"), 1);
}

#[test]
fn open_turns_foreign_keys_on() {
    let dir = tmp();
    let (db, _) = open_fresh(&dir);
    assert_eq!(pragma::<i64>(&db, "foreign_keys"), 1);
}

#[test]
fn open_sets_busy_timeout_to_five_seconds() {
    let dir = tmp();
    let (db, _) = open_fresh(&dir);
    assert_eq!(pragma::<i64>(&db, "busy_timeout"), 5000);
}

#[test]
fn deferred_checkpoints_keep_many_writes_within_the_wal_bound() {
    let dir = tmp();
    let (db, _) = open_fresh(&dir);
    db.defer_checkpoint().unwrap();
    db.connection()
        .execute_batch("CREATE TABLE wal_load (value BLOB NOT NULL) STRICT")
        .unwrap();
    let wal = db_path(&dir).with_extension("db-wal");
    let mut insert = db
        .connection()
        .prepare("INSERT INTO wal_load VALUES (zeroblob(262144))")
        .unwrap();
    for _ in 0..320 {
        insert.execute([]).unwrap();
        assert!(
            fs::metadata(&wal).unwrap().len() <= 64 * 1024 * 1024,
            "automatic checkpoints must bound WAL growth during a session"
        );
    }
    assert_eq!(pragma::<i64>(&db, "journal_size_limit"), 64 * 1024 * 1024);
    db.checkpoint().unwrap();
    assert_eq!(fs::metadata(wal).unwrap().len(), 0);
}
