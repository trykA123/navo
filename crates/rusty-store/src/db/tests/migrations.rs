use super::*;

#[test]
fn migrations_are_not_empty() {
    assert!(!MIGRATIONS.is_empty());
}

#[test]
fn every_migration_has_sql() {
    let empty: Vec<usize> = (0..MIGRATIONS.len())
        .filter(|&i| MIGRATIONS[i].trim().is_empty())
        .collect();
    assert!(empty.is_empty(), "empty migrations: {empty:?}");
}

#[test]
fn version_counts_the_migrations() {
    assert_eq!(Db::VERSION as usize, MIGRATIONS.len());
}

#[test]
fn first_migration_creates_every_v1_table() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(MIGRATIONS[0]).unwrap();
    assert_eq!(table_names(&conn), V1_TABLES);
}

#[test]
fn version_is_three() {
    assert_eq!(Db::VERSION, 3);
}
