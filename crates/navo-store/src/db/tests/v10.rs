use super::*;

#[test]
fn migration_from_v9_keeps_every_retained_row_and_starts_attempts_at_zero() {
    let dir = tmp();
    let path = db_path(&dir);
    build_at_version(&path, 9);
    let conn = Connection::open(&path).unwrap();
    for state in ["ready", "purging", "restoring", "purged"] {
        conn.execute(
            "INSERT INTO retained_entry (state, record, target) VALUES (?1, X'ff', X'aa')",
            [state],
        )
        .unwrap();
    }
    drop(conn);
    let (db, report) = Db::open(&path, false).unwrap();
    assert_eq!((report.from_version, report.to_version), (9, 10));
    let mut statement = db
        .connection()
        .prepare(
            "SELECT state, record, target, maintenance_attempts FROM retained_entry ORDER BY id",
        )
        .unwrap();
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let states: Vec<_> = rows.iter().map(|row| row.0.as_str()).collect();
    assert_eq!(states, ["ready", "purging", "restoring", "purged"]);
    assert!(
        rows.iter()
            .all(|row| row.1 == [0xff] && row.2 == [0xaa] && row.3 == 0)
    );
    let negative = db.connection().execute(
        "UPDATE retained_entry SET maintenance_attempts = -1 WHERE id = 1",
        [],
    );
    assert!(negative.is_err());
}
