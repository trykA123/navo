use std::collections::BTreeMap;

use super::*;

fn rows(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

fn ledger(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    rows(pairs).into_iter().collect()
}

fn written_at(db: &Db, key: &str) -> i64 {
    db.connection()
        .query_row(
            "SELECT written_at FROM settings_defaults WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn a_fresh_database_has_an_empty_ledger() {
    let db = Db::open_in_memory().unwrap();
    assert_eq!(db.settings_defaults().unwrap(), BTreeMap::new());
}

#[test]
fn recorded_defaults_read_back() {
    let db = Db::open_in_memory().unwrap();
    let written = rows(&[("files.sort", "\"name\""), ("appearance.font-size", "13")]);
    db.record_settings_defaults(&written, 100).unwrap();
    assert_eq!(
        db.settings_defaults().unwrap(),
        ledger(&[("files.sort", "\"name\""), ("appearance.font-size", "13")])
    );
}

#[test]
fn recording_a_key_again_replaces_its_value_and_time() {
    let db = Db::open_in_memory().unwrap();
    db.record_settings_defaults(&rows(&[("files.sort", "\"name\"")]), 100)
        .unwrap();
    db.record_settings_defaults(&rows(&[("files.sort", "\"date\"")]), 200)
        .unwrap();
    assert_eq!(
        db.settings_defaults().unwrap(),
        ledger(&[("files.sort", "\"date\"")])
    );
    assert_eq!(written_at(&db, "files.sort"), 200);
}

#[test]
fn recording_keeps_the_other_keys() {
    let db = Db::open_in_memory().unwrap();
    db.record_settings_defaults(&rows(&[("files.sort", "\"name\"")]), 100)
        .unwrap();
    db.record_settings_defaults(&rows(&[("files.show-system", "false")]), 200)
        .unwrap();
    assert_eq!(db.settings_defaults().unwrap().len(), 2);
    assert_eq!(written_at(&db, "files.sort"), 100);
}

#[test]
fn recording_nothing_is_fine() {
    let db = Db::open_in_memory().unwrap();
    db.record_settings_defaults(&[], 100).unwrap();
    assert!(db.settings_defaults().unwrap().is_empty());
}

#[test]
fn v2_fixture_migrates_to_current_version() {
    let dir = tmp();
    let path = db_path(&dir);
    build_from_sql(&path, V2_SQL);
    let (db, report) = Db::open(&path, false).unwrap();
    assert_eq!(
        (report.from_version, db.version().unwrap()),
        (2, Db::VERSION)
    );
    assert_eq!(table_names(db.connection()), TABLES);
}

#[test]
fn v2_fixture_rows_survive_and_the_ledger_starts_empty() {
    let dir = tmp();
    let path = db_path(&dir);
    build_from_sql(&path, V2_SQL);
    let before = row_counts(&Connection::open(&path).unwrap());
    let (db, _) = Db::open(&path, false).unwrap();
    assert_eq!(row_counts(db.connection()), before);
    let viewer: i64 = db
        .connection()
        .query_row("SELECT count(*) FROM viewer_memory", [], |row| row.get(0))
        .unwrap();
    assert_eq!(viewer, 1);
    assert!(db.settings_defaults().unwrap().is_empty());
}

#[test]
fn the_ledger_survives_a_reopen() {
    let dir = tmp();
    let path = db_path(&dir);
    {
        let (db, _) = Db::open(&path, false).unwrap();
        db.record_settings_defaults(&rows(&[("files.sort", "\"name\"")]), 100)
            .unwrap();
    }
    let (db, _) = Db::open(&path, false).unwrap();
    assert_eq!(
        db.settings_defaults().unwrap(),
        ledger(&[("files.sort", "\"name\"")])
    );
}
