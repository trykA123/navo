use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use tempfile::TempDir;

use super::*;

mod basic;
mod budget;
mod open;
mod props;

fn mem() -> CacheDb {
    CacheDb::open_in_memory().unwrap()
}

fn mem_with_budget(table: &str, bytes: u64) -> CacheDb {
    let mut cache = mem();
    cache.set_budget(table, bytes);
    cache
}

fn tmp() -> TempDir {
    TempDir::new().unwrap()
}

fn cache_path(dir: &TempDir) -> PathBuf {
    dir.path().join("cache.db")
}

fn corrupt_path(dir: &TempDir, n: u32) -> PathBuf {
    dir.path().join(format!("cache.db.corrupt-{n}"))
}

fn has(cache: &CacheDb, table: &str, key: &[u8]) -> bool {
    cache.get(table, key).unwrap().is_some()
}
