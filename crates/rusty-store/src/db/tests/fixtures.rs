use super::*;

fn open_fixture(dir: &TempDir, sql: &str) -> (Db, OpenReport) {
    let path = db_path(dir);
    build_from_sql(&path, sql);
    Db::open(&path, false).unwrap()
}

#[test]
fn v0_fixture_migrates_to_current_version() {
    let dir = tmp();
    let (db, _) = open_fixture(&dir, V0_SQL);
    assert_eq!(db.version().unwrap(), Db::VERSION);
}

#[test]
fn v0_fixture_reports_its_starting_version() {
    let dir = tmp();
    let (_, report) = open_fixture(&dir, V0_SQL);
    assert_eq!((report.from_version, report.to_version), (0, Db::VERSION));
}

#[test]
fn v0_fixture_gets_every_table() {
    let dir = tmp();
    let (db, _) = open_fixture(&dir, V0_SQL);
    assert_eq!(table_names(db.connection()), TABLES);
}

#[test]
fn v1_fixture_migrates_to_current_version() {
    let dir = tmp();
    let (db, _) = open_fixture(&dir, V1_SQL);
    assert_eq!(db.version().unwrap(), Db::VERSION);
}

#[test]
fn v1_fixture_reports_its_starting_version() {
    let dir = tmp();
    let (_, report) = open_fixture(&dir, V1_SQL);
    assert_eq!((report.from_version, report.to_version), (1, Db::VERSION));
}

#[test]
fn v1_fixture_gets_every_table() {
    let dir = tmp();
    let (db, _) = open_fixture(&dir, V1_SQL);
    assert_eq!(table_names(db.connection()), TABLES);
}

#[test]
fn v1_fixture_rows_survive_open() {
    let dir = tmp();
    let path = db_path(&dir);
    build_from_sql(&path, V1_SQL);
    let before = row_counts(&Connection::open(&path).unwrap());
    let (db, _) = Db::open(&path, false).unwrap();
    assert_eq!(row_counts(db.connection()), before);
}

#[test]
fn v1_fixture_job_steps_keep_their_detail() {
    let dir = tmp();
    let (db, _) = open_fixture(&dir, V1_SQL);
    let detail: Option<Vec<u8>> = db
        .connection()
        .query_row(
            "SELECT detail FROM job_step WHERE job_id = 1 AND seq = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(detail, Some(vec![0xff]));
}
