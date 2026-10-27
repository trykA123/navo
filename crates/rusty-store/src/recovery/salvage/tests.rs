use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use rusty_core::{OpKind, OperationPlan, Totals};

use super::*;
use crate::recovery::SalvageOutcome;
use crate::{CollectionKind, JobJournal, NewCollection, quarantine_database};

fn populated(path: &Path) {
    let (db, _) = Db::open(path, false).unwrap();
    db.connection()
        .execute_batch(
            "INSERT INTO tags VALUES ('/sample/a', 'keep');
         INSERT INTO module_kv VALUES ('module', 'key', X'0102');
         INSERT INTO settings_defaults VALUES ('files.hidden', 'false', 1);
         INSERT INTO command_use VALUES ('open', 1.5, 2);
         INSERT INTO rename_preset VALUES ('saved', '{}', 3);
         WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 2000)
         INSERT INTO history(location, visited_at)
         SELECT printf('/sample/history/%05d/long/path/for/a/page', i), i FROM n;",
        )
        .unwrap();
}

fn corrupt_table(path: &Path, table: &str) {
    let conn = Connection::open(path).unwrap();
    let page: u32 = conn
        .query_row(
            "SELECT rootpage FROM sqlite_schema WHERE name = ?1",
            [table],
            |row| row.get(0),
        )
        .unwrap();
    let page = usize::try_from(page).unwrap();
    drop(conn);
    let mut bytes = fs::read(path).unwrap();
    for (i, byte) in bytes[(page - 1) * 4096..page * 4096].iter_mut().enumerate() {
        *byte = (i * 31 + 7) as u8;
    }
    fs::write(path, bytes).unwrap();
}

fn quarantined(path: &Path) -> PathBuf {
    quarantine_database(path)
        .unwrap()
        .unwrap()
        .database
        .unwrap()
}

fn count(db: &Db, table: &str) -> i64 {
    db.connection()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn populate_collections(db: &Db) {
    for (name, kind, query, base) in [
        (
            "Sources",
            CollectionKind::Search,
            Some("ext:rs"),
            Some("/work"),
        ),
        ("Archive", CollectionKind::Snapshot, None, Some("/archive")),
        ("Picked", CollectionKind::Manual, None, None),
    ] {
        let id = db
            .create_collection(
                &NewCollection {
                    name,
                    kind,
                    query,
                    base,
                },
                100,
            )
            .unwrap();
        if kind != CollectionKind::Search {
            db.add_collection_items(id, &["/z-last", "/a-first"], 200)
                .unwrap();
            db.add_collection_items(id, &["/m-middle"], 300).unwrap();
        }
    }
}

#[test]
fn collections_and_ordered_items_survive_damage_in_another_table() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    populated(&path);
    let (db, _) = Db::open(&path, false).unwrap();
    populate_collections(&db);
    let collections = db.list_collections().unwrap();
    let items = collections
        .iter()
        .map(|collection| (collection.id, db.collection_items(collection.id).unwrap()))
        .collect::<Vec<_>>();
    drop(db);
    corrupt_table(&path, "history");
    let source = quarantined(&path);
    let fresh = Db::open_in_memory().unwrap();
    let report = salvage_database(&source, &fresh).unwrap();
    assert_eq!(fresh.list_collections().unwrap(), collections);
    for (id, expected) in items {
        assert_eq!(fresh.collection_items(id).unwrap(), expected);
    }
    for (name, rows) in [("collections", 3), ("collection_items", 6)] {
        assert!(report.tables.iter().any(|table| table.name == name
            && table.outcome == SalvageOutcome::Copied { rows }));
    }
}

#[test]
fn damaged_or_missing_collection_tables_are_reported_without_aborting_salvage() {
    for table in ["collections", "collection_items"] {
        for missing in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("ferrite.db");
            populated(&path);
            let (db, _) = Db::open(&path, false).unwrap();
            populate_collections(&db);
            drop(db);
            if missing {
                Connection::open(&path)
                    .unwrap()
                    .execute_batch(&format!("PRAGMA foreign_keys=OFF; DROP TABLE {table};"))
                    .unwrap();
            } else {
                corrupt_table(&path, table);
            }
            let source = quarantined(&path);
            let fresh = Db::open_in_memory().unwrap();
            let report = salvage_database(&source, &fresh).unwrap();
            assert!(report.tables.iter().any(
                |row| row.name == table && matches!(row.outcome, SalvageOutcome::Reset { .. })
            ));
            assert_eq!(count(&fresh, "tags"), 1);
            assert_eq!(count(&fresh, "history"), 2000);
            assert_eq!(count(&fresh, "collection_items"), 0);
            assert_eq!(
                count(&fresh, "collections"),
                if table == "collections" { 0 } else { 3 }
            );
        }
    }
}

fn plan() -> Vec<u8> {
    serde_json::to_vec(&OperationPlan {
        kind: OpKind::Copy,
        steps: Vec::new(),
        conflicts: Vec::new(),
        risks: Vec::new(),
        totals: Totals {
            files: 0,
            folders: 0,
            bytes: 0,
        },
    })
    .unwrap()
}

#[test]
fn damaged_history_resets_only_history_and_preserves_user_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    populated(&path);
    corrupt_table(&path, "history");
    let source = quarantined(&path);
    let before = fs::read(&source).unwrap();
    let (fresh, _) = Db::open(&path, false).unwrap();
    let report = salvage_database(&source, &fresh).unwrap();
    for table in [
        "tags",
        "module_kv",
        "settings_defaults",
        "command_use",
        "rename_preset",
    ] {
        assert_eq!(count(&fresh, table), 1, "{table}");
    }
    assert_eq!(count(&fresh, "history"), 0);
    assert!(
        report.tables.iter().any(|table| table.name == "history"
            && matches!(table.outcome, SalvageOutcome::Reset { .. }))
    );
    assert_eq!(fs::read(source).unwrap(), before);
}

#[test]
fn a_valid_pending_journal_survives_damage_in_another_table() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    populated(&path);
    let (db, _) = Db::open(&path, false).unwrap();
    let mut journal = JobJournal::new(db).unwrap();
    journal.create_job("copy", &plan(), 0, 7).unwrap();
    drop(journal);
    corrupt_table(&path, "history");
    let source = quarantined(&path);
    let (fresh, _) = Db::open(&path, false).unwrap();
    let report = salvage_database(&source, &fresh).unwrap();
    assert!(report.jobs.is_empty());
    assert_eq!(
        JobJournal::new(fresh).unwrap().unfinished().unwrap().len(),
        1
    );
}

#[test]
fn invalid_restore_plans_reject_every_journal_table_and_keep_manual_details() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    populated(&path);
    let (db, _) = Db::open(&path, false).unwrap();
    db.connection()
        .execute_batch(
            "INSERT INTO job VALUES (1, 'move', 'restore_pending', 77, X'00');
         INSERT INTO job_step VALUES (1, 0, 'done', X'01');
         INSERT INTO undo VALUES (1, 1, 78, X'0203');",
        )
        .unwrap();
    drop(db);
    let source = quarantined(&path);
    let fresh = Db::open_in_memory().unwrap();
    let report = salvage_database(&source, &fresh).unwrap();
    for table in ["job", "job_step", "undo"] {
        assert_eq!(count(&fresh, table), 0);
        assert!(report.tables.iter().any(|row| row.name == table && matches!(row.outcome, SalvageOutcome::Reset { .. })));
    }
    assert_eq!(report.jobs.len(), 1);
    assert_eq!(report.jobs[0].kind.as_deref(), Some("move"));
    assert_eq!(report.jobs[0].created_at, Some(77));
    assert_eq!(report.jobs[0].held, [vec![2, 3]]);
    assert_eq!(count(&fresh, "tags"), 1);
}

#[test]
fn journal_foreign_key_damage_copies_no_journal_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    populated(&path);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "PRAGMA foreign_keys=OFF; INSERT INTO job_step VALUES (999, 0, 'done', NULL);",
    )
    .unwrap();
    drop(conn);
    let source = quarantined(&path);
    let fresh = Db::open_in_memory().unwrap();
    let report = salvage_database(&source, &fresh).unwrap();
    assert_eq!(count(&fresh, "job_step"), 0);
    assert!(
        report
            .tables
            .iter()
            .any(|table| table.name == "job"
                && matches!(table.outcome, SalvageOutcome::Reset { .. }))
    );
}

#[test]
fn a_nonempty_destination_is_rejected_before_any_copy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    populated(&path);
    let source = quarantined(&path);
    let fresh = Db::open_in_memory().unwrap();
    fresh
        .connection()
        .execute("INSERT INTO tags VALUES ('/keep', 'existing')", [])
        .unwrap();
    assert!(matches!(
        salvage_database(&source, &fresh),
        Err(RecoveryError::DestinationNotEmpty("tags"))
    ));
    assert_eq!(count(&fresh, "tags"), 1);
    assert_eq!(count(&fresh, "module_kv"), 0);
}

#[test]
fn an_unreadable_schema_returns_reset_and_manual_recovery_information() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.db");
    fs::write(&path, [0x5a; 4096]).unwrap();
    let fresh = Db::open_in_memory().unwrap();
    let report = salvage_database(&path, &fresh).unwrap();
    assert_eq!(report.tables.len(), 15);
    assert!(
        report
            .tables
            .iter()
            .all(|row| matches!(row.outcome, SalvageOutcome::Reset { .. }))
    );
    assert!(!report.issues.is_empty());
}

#[test]
fn read_only_salvage_keeps_database_and_hot_wal_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    populated(&path);
    let conn = Connection::open(&path).unwrap();
    conn.set_db_config(
        rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
        true,
    )
    .unwrap();
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; INSERT INTO tags VALUES ('/wal-only', 'committed');",
    )
    .unwrap();
    drop(conn);
    let set = quarantine_database(&path).unwrap().unwrap();
    let source = set.database.unwrap();
    let wal = set.wal.unwrap();
    let before = (fs::read(&source).unwrap(), fs::read(&wal).unwrap());
    let fresh = Db::open_in_memory().unwrap();
    salvage_database(&source, &fresh).unwrap();
    assert_eq!(count(&fresh, "tags"), 2);
    assert_eq!((fs::read(source).unwrap(), fs::read(wal).unwrap()), before);
}

#[test]
fn journal_copy_rolls_back_all_rows_if_a_late_constraint_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    populated(&path);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "INSERT INTO job VALUES (1, 'copy', 'done', 1, X'00');
        PRAGMA foreign_keys=OFF; INSERT INTO undo VALUES (1, 999, 2, X'01');",
    )
    .unwrap();
    drop(conn);
    let source = quarantined(&path);
    let fresh = Db::open_in_memory().unwrap();
    salvage_database(&source, &fresh).unwrap();
    assert_eq!(count(&fresh, "job"), 0);
    assert_eq!(count(&fresh, "undo"), 0);
    assert_eq!(count(&fresh, "tags"), 1);
}

#[test]
fn a_damaged_step_table_keeps_readable_job_and_held_details() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    populated(&path);
    let (db, _) = Db::open(&path, false).unwrap();
    let mut journal = JobJournal::new(db).unwrap();
    let job = journal.create_job("copy", &plan(), 1, 91).unwrap();
    journal
        .record_held(job, b"manual restore record", 92)
        .unwrap();
    drop(journal);
    corrupt_table(&path, "job_step");
    let source = quarantined(&path);
    let fresh = Db::open_in_memory().unwrap();
    let report = salvage_database(&source, &fresh).unwrap();
    assert_eq!(count(&fresh, "job"), 0);
    assert_eq!(count(&fresh, "job_step"), 0);
    assert_eq!(count(&fresh, "undo"), 0);
    assert_eq!(report.jobs[0].created_at, Some(91));
    assert_eq!(report.jobs[0].held, [b"manual restore record".to_vec()]);
    assert_eq!(count(&fresh, "tags"), 1);
}

#[test]
fn a_row_read_failure_rolls_back_the_entire_table() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    populated(&path);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "DELETE FROM history;
        INSERT INTO history VALUES (1, '/readable', 1);
        INSERT INTO history VALUES (2, CAST(X'80' AS TEXT), 2);",
    )
    .unwrap();
    drop(conn);
    let source = quarantined(&path);
    let fresh = Db::open_in_memory().unwrap();
    let report = salvage_database(&source, &fresh).unwrap();
    assert_eq!(count(&fresh, "history"), 0);
    assert!(report.tables.iter().any(|row| row.name == "history" && matches!(row.outcome, SalvageOutcome::Reset { .. })));
    assert_eq!(count(&fresh, "tags"), 1);
}

#[test]
fn foreign_keys_remain_enforced_when_the_destination_disabled_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    populated(&path);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "PRAGMA foreign_keys=OFF; INSERT INTO job_step VALUES (999, 0, 'done', NULL);",
    )
    .unwrap();
    drop(conn);
    let source = quarantined(&path);
    let fresh = Db::open_in_memory().unwrap();
    fresh
        .connection()
        .pragma_update(None, "foreign_keys", false)
        .unwrap();
    salvage_database(&source, &fresh).unwrap();
    assert_eq!(count(&fresh, "job_step"), 0);
}

#[test]
fn newer_sources_are_rejected_without_copying_known_tables() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    populated(&path);
    let conn = Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", Db::VERSION + 1)
        .unwrap();
    drop(conn);
    let source = quarantined(&path);
    let fresh = Db::open_in_memory().unwrap();
    assert!(matches!(
        salvage_database(&source, &fresh),
        Err(RecoveryError::UnsupportedVersion { .. })
    ));
    assert_eq!(count(&fresh, "tags"), 0);
}

#[test]
fn older_tables_use_current_defaults_for_new_columns() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(include_str!("../../../tests/fixtures/db/v1.sql"))
        .unwrap();
    conn.execute(
        "INSERT INTO frecency(location, score, last_at) VALUES ('/legacy', 1.0, 3)",
        [],
    )
    .unwrap();
    drop(conn);
    let source = quarantined(&path);
    let fresh = Db::open_in_memory().unwrap();
    salvage_database(&source, &fresh).unwrap();
    let is_folder: bool = fresh
        .connection()
        .query_row(
            "SELECT is_folder FROM frecency WHERE location='/legacy'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(is_folder);
}

#[cfg(unix)]
#[test]
fn unreadable_sources_return_an_error_instead_of_resetting_data() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    populated(&path);
    let source = quarantined(&path);
    fs::set_permissions(&source, fs::Permissions::from_mode(0o000)).unwrap();
    let fresh = Db::open_in_memory().unwrap();
    assert!(matches!(
        salvage_database(&source, &fresh),
        Err(RecoveryError::Source { .. })
    ));
    assert_eq!(count(&fresh, "tags"), 0);
}

#[test]
fn an_orphan_wal_cannot_populate_a_fresh_or_older_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    let older = dir.path().join("ferrite.db.corrupt-1");
    populated(&older);
    let (source, _) = Db::open(&path, false).unwrap();
    source
        .connection()
        .set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        )
        .unwrap();
    source
        .connection()
        .execute_batch(
            "CREATE TABLE orphan_payload(value); INSERT INTO orphan_payload VALUES (42);",
        )
        .unwrap();
    drop(source);
    fs::rename(&path, dir.path().join("separated-original.db")).unwrap();
    let report = quarantine_database(&path).unwrap().unwrap();
    assert!(report.database.is_none());
    assert!(report.wal.is_some());
    let (fresh, _) = Db::open(&path, false).unwrap();
    let (older, _) = Db::open(&older, true).unwrap();
    for db in [&fresh, &older] {
        let present: bool = db
            .connection()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='orphan_payload')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!present);
    }
    assert_eq!(count(&older, "tags"), 1);
}

#[test]
fn a_blob_in_a_text_column_resets_the_group_and_keeps_the_adapter_usable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    populated(&path);
    let conn = Connection::open(&path).unwrap();
    conn.execute("INSERT INTO rename_preset VALUES ('broken', X'00', 1)", [])
        .unwrap();
    drop(conn);
    let source = quarantined(&path);
    let fresh = Db::open_in_memory().unwrap();
    let report = salvage_database(&source, &fresh).unwrap();
    assert!(fresh.presets().is_ok());
    assert_eq!(count(&fresh, "rename_preset"), 0);
    assert_eq!(count(&fresh, "tags"), 1);
    assert!(
        report
            .tables
            .iter()
            .any(|table| table.name == "rename_preset"
                && matches!(table.outcome, SalvageOutcome::Reset { .. }))
    );
}

#[test]
fn malformed_scalar_and_blob_fields_reset_their_groups() {
    for (table, sql) in [
        (
            "command_use",
            "INSERT INTO command_use VALUES ('broken', 1, 'invalid time')",
        ),
        (
            "module_kv",
            "INSERT INTO module_kv VALUES ('broken', 'key', 'not a blob')",
        ),
        (
            "frecency",
            "INSERT INTO frecency VALUES ('/broken', X'00', 1, 1)",
        ),
        (
            "rename_preset",
            "INSERT INTO rename_preset VALUES (NULL, '{}', 1)",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ferrite.db");
        populated(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(sql).unwrap();
        drop(conn);
        let source = quarantined(&path);
        let fresh = Db::open_in_memory().unwrap();
        let report = salvage_database(&source, &fresh).unwrap();
        assert_eq!(count(&fresh, table), 0, "{table}");
        assert_eq!(count(&fresh, "tags"), 1);
        assert!(report.tables.iter().any(|row| row.name == table && matches!(row.outcome, SalvageOutcome::Reset { .. })));
    }
}
