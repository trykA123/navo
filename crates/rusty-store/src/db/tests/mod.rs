use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use tempfile::TempDir;

use super::*;

mod fixtures;
mod fresh;
mod guards;
mod memory;
mod migrations;
mod props;

const TABLES: [&str; 7] = [
    "frecency",
    "history",
    "job",
    "job_step",
    "module_kv",
    "tags",
    "undo",
];

const V0_SQL: &str = include_str!("../../../tests/fixtures/db/v0.sql");
const V1_SQL: &str = include_str!("../../../tests/fixtures/db/v1.sql");

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

fn db_path(dir: &TempDir) -> PathBuf {
    dir.path().join("ferrite.db")
}

fn build_from_sql(path: &Path, sql: &str) {
    Connection::open(path).unwrap().execute_batch(sql).unwrap();
}

fn build_at_version(path: &Path, version: u32) {
    let conn = Connection::open(path).unwrap();
    for (reached, sql) in (1..=version).zip(MIGRATIONS) {
        conn.execute_batch(sql).unwrap();
        conn.pragma_update(None, "user_version", reached).unwrap();
    }
}

fn set_user_version(path: &Path, version: u32) {
    Connection::open(path)
        .unwrap()
        .pragma_update(None, "user_version", version)
        .unwrap();
}

fn table_names(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap();
    stmt.query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn row_counts(conn: &Connection) -> Vec<(&'static str, i64)> {
    TABLES
        .iter()
        .map(|table| {
            let sql = format!("SELECT count(*) FROM {table}");
            (*table, conn.query_row(&sql, [], |row| row.get(0)).unwrap())
        })
        .collect()
}
