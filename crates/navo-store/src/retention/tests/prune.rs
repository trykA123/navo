use super::*;

const NOW: i64 = 100 * DAY_MS;

fn job(catalog: &RetentionCatalog, id: i64) -> Option<crate::JobId> {
    catalog
        .db
        .connection()
        .execute(
            "INSERT INTO job VALUES (?1, 'delete', 'done', 0, X'00')",
            [id],
        )
        .unwrap();
    Some(crate::JobId(id))
}

fn live(catalog: &mut RetentionCatalog, job: Option<crate::JobId>) -> RetentionId {
    let mut tx = catalog.transaction().unwrap();
    let entry = tx.insert(&record(), job).unwrap();
    tx.commit().unwrap();
    entry.id
}

fn terminal(
    catalog: &mut RetentionCatalog,
    job: Option<crate::JobId>,
    (age, acknowledged): (i64, bool),
) -> RetentionId {
    let mut tx = catalog.transaction().unwrap();
    let mut entry = tx.insert(&record(), job).unwrap();
    entry.state = RetainedState::Aborted;
    tx.save(&entry, RetainedState::Preparing).unwrap();
    tx.commit().unwrap();
    if acknowledged {
        catalog.acknowledge_cleanup(entry.id).unwrap();
    }
    catalog
        .db
        .connection()
        .execute(
            "UPDATE retained_entry SET terminal_ms = ?2 WHERE id = ?1",
            rusqlite::params![entry.id.0, NOW - age],
        )
        .unwrap();
    entry.id
}

fn terminal_ms(catalog: &RetentionCatalog, id: RetentionId) -> Option<i64> {
    catalog
        .db
        .connection()
        .query_row(
            "SELECT terminal_ms FROM retained_entry WHERE id = ?1",
            [id.0],
            |row| row.get(0),
        )
        .unwrap()
}

fn present(catalog: &RetentionCatalog, id: RetentionId) -> bool {
    catalog.get(id).is_ok()
}

fn wall_ms() -> i64 {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    i64::try_from(elapsed.as_millis()).unwrap()
}

#[test]
fn prune_a_transition_to_a_terminal_state_sets_terminal_ms() {
    let mut catalog = catalog();
    let mut entry = insert(&mut catalog);
    assert_eq!(terminal_ms(&catalog, entry.id), None);
    let before = wall_ms();
    entry.state = RetainedState::Aborted;
    let mut tx = catalog.transaction().unwrap();
    tx.save(&entry, RetainedState::Preparing).unwrap();
    tx.commit().unwrap();
    let stamped = terminal_ms(&catalog, entry.id).unwrap();
    assert!((before - 1_000..=wall_ms() + 1_000).contains(&stamped));
}

#[test]
fn prune_a_live_transition_leaves_terminal_ms_empty() {
    let mut catalog = catalog();
    let mut entry = insert(&mut catalog);
    entry.record.container_identity = Some(entry.record.parent_identity);
    entry.state = RetainedState::Ready;
    let mut tx = catalog.transaction().unwrap();
    tx.save(&entry, RetainedState::Preparing).unwrap();
    tx.commit().unwrap();
    assert_eq!(terminal_ms(&catalog, entry.id), None);
}

#[test]
fn prune_a_live_row_refuses_a_terminal_time() {
    let mut catalog = catalog();
    let entry = insert(&mut catalog);
    assert!(
        catalog
            .db
            .connection()
            .execute(
                "UPDATE retained_entry SET terminal_ms = 0 WHERE id = ?1",
                [entry.id.0],
            )
            .is_err()
    );
}

#[test]
fn prune_removes_old_acknowledged_rows_only() {
    let mut catalog = catalog();
    let old = terminal(&mut catalog, None, (31 * DAY_MS, true));
    let unacknowledged = terminal(&mut catalog, None, (31 * DAY_MS, false));
    let young = terminal(&mut catalog, None, (29 * DAY_MS, true));
    let edge = terminal(&mut catalog, None, (30 * DAY_MS, true));
    let active = live(&mut catalog, None);
    assert_eq!(catalog.prune_terminal(NOW).unwrap(), 1);
    assert!(!present(&catalog, old));
    for kept in [unacknowledged, young, edge, active] {
        assert!(present(&catalog, kept));
    }
    assert_eq!(catalog.prune_terminal(NOW).unwrap(), 0);
}

#[test]
fn prune_keeps_rows_whose_job_still_has_a_live_row() {
    let mut catalog = catalog();
    let shared = job(&catalog, 1);
    let alone = job(&catalog, 2);
    let held = terminal(&mut catalog, shared, (31 * DAY_MS, true));
    let active = live(&mut catalog, shared);
    let free = terminal(&mut catalog, alone, (31 * DAY_MS, true));
    assert_eq!(catalog.prune_terminal(NOW).unwrap(), 1);
    assert!(present(&catalog, held));
    assert!(present(&catalog, active));
    assert!(!present(&catalog, free));
}

#[test]
fn prune_removes_at_most_one_thousand_rows_per_call() {
    let mut catalog = catalog();
    {
        let mut insert = catalog
            .db
            .connection()
            .prepare(
                "INSERT INTO retained_entry (state, record, cleanup_pending, terminal_ms)
                 VALUES ('purged', X'ff', 0, 0)",
            )
            .unwrap();
        for _ in 0..1_001 {
            insert.execute([]).unwrap();
        }
    }
    assert_eq!(catalog.prune_terminal(NOW).unwrap(), 1_000);
    assert_eq!(catalog.prune_terminal(NOW).unwrap(), 1);
    assert_eq!(catalog.prune_terminal(NOW).unwrap(), 0);
}

#[test]
fn prune_refuses_a_negative_clock() {
    let mut catalog = catalog();
    assert!(catalog.prune_terminal(-1).is_err());
}

fn salvaged(source: &std::path::Path, id: RetentionId) -> Option<i64> {
    let fresh = Db::open_in_memory().unwrap();
    crate::salvage_database(source, &fresh).unwrap();
    let catalog = RetentionCatalog::new(fresh).unwrap();
    assert_eq!(catalog.get(id).unwrap().state, RetainedState::Aborted);
    terminal_ms(&catalog, id)
}

fn aborted_source(path: &std::path::Path) -> RetentionId {
    let (db, _) = Db::open(path, false).unwrap();
    let mut catalog = RetentionCatalog::new(db).unwrap();
    terminal(&mut catalog, None, (31 * DAY_MS, true))
}

#[test]
fn prune_salvage_keeps_the_terminal_time_of_a_current_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.db");
    let id = aborted_source(&path);
    assert_eq!(salvaged(&path, id), Some(NOW - 31 * DAY_MS));
}

#[test]
fn prune_salvage_stamps_terminal_rows_from_a_source_without_terminal_times() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.db");
    let id = aborted_source(&path);
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "DROP INDEX retained_entry_prunable;
         ALTER TABLE retained_entry DROP COLUMN terminal_ms;
         PRAGMA user_version = 8;",
    )
    .unwrap();
    drop(conn);
    let before = wall_ms();
    let stamped = salvaged(&path, id).unwrap();
    assert!((before - 1_000..=wall_ms() + 1_000).contains(&stamped));
}
