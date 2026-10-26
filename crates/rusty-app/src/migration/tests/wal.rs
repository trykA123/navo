use std::path::Path;

use rusty_store::Db;

use super::super::{Outcome, RealFs};
use super::fixture::Fixture;

fn read_row(path: &Path) -> Result<String, String> {
    let (db, _) = Db::open(path, true).map_err(|error| error.to_string())?;
    db.connection()
        .query_row("SELECT v FROM t", [], |row| row.get(0))
        .map_err(|error| error.to_string())
}

fn crashed_ferrite_files(dir: &Path) -> (Vec<u8>, Vec<u8>) {
    let path = dir.join("ferrite.db");
    let (db, _) = Db::open(&path, false).unwrap();
    db.connection()
        .execute_batch(
            "PRAGMA wal_autocheckpoint = 0;
             CREATE TABLE t (v TEXT);
             INSERT INTO t VALUES ('kept in the wal');",
        )
        .unwrap();
    let files = (
        std::fs::read(&path).unwrap(),
        std::fs::read(dir.join("ferrite.db-wal")).unwrap(),
    );
    drop(db);
    files
}

#[test]
fn rows_that_live_only_in_the_wal_survive_the_migration() {
    let scratch = tempfile::tempdir().unwrap();
    let (db, wal) = crashed_ferrite_files(scratch.path());
    assert!(!wal.is_empty());

    let control = tempfile::tempdir().unwrap();
    std::fs::write(control.path().join("ferrite.db"), &db).unwrap();
    assert!(read_row(&control.path().join("ferrite.db")).is_err());

    for fixture in [Fixture::user(), Fixture::profile()] {
        let data = if fixture.in_place() {
            "p/data"
        } else {
            "local/Ferrite"
        };
        fixture.write(&format!("{data}/ferrite.db"), &db);
        fixture.write(&format!("{data}/ferrite.db-wal"), &wal);
        std::fs::remove_file(fixture.path(&format!("{data}/ferrite.db-shm"))).unwrap();
        assert!(matches!(fixture.run(&RealFs), Outcome::Done));
        let row = read_row(&fixture.new.data.join("navo.db"));
        assert_eq!(row.as_deref(), Ok("kept in the wal"));
    }
}
