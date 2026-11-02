use super::*;

#[test]
fn in_memory_db_is_at_current_version() {
    let db = Db::open_in_memory().unwrap();
    assert_eq!(db.version().unwrap(), Db::VERSION);
}

#[test]
fn in_memory_db_enforces_foreign_keys() {
    let db = Db::open_in_memory().unwrap();
    let insert = db.connection().execute(
        "INSERT INTO job_step (job_id, seq, state) VALUES (42, 0, 'pending')",
        [],
    );
    assert!(insert.is_err(), "orphan job_step was accepted");
}
