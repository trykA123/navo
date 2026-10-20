use rusqlite::{Transaction, params};

use super::{Db, DbError};

const HISTORY_MAX: i64 = 10_000;
const DECAY_ABOVE: f64 = 10_000.0;
const DECAY: f64 = 0.9;
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;
const WEEK_MS: i64 = 7 * DAY_MS;
const WEIGHT: &str = "score * CASE WHEN ?1 - last_at < ?2 THEN 4.0 WHEN ?1 - last_at < ?3 THEN 2.0
    WHEN ?1 - last_at < ?4 THEN 0.5 ELSE 0.25 END";

#[derive(Debug, Clone, PartialEq)]
pub struct Frecent {
    pub location: String,
    pub is_folder: bool,
    pub weight: f64,
}

fn limit_of(limit: usize) -> i64 {
    i64::try_from(limit).unwrap_or(i64::MAX)
}

fn decay(transaction: &Transaction<'_>, table: &str) -> Result<(), DbError> {
    let total: f64 =
        transaction.query_row(&format!("SELECT total(score) FROM {table}"), [], |row| {
            row.get(0)
        })?;
    if total >= DECAY_ABOVE {
        transaction.execute(&format!("UPDATE {table} SET score = score * ?1"), [DECAY])?;
        transaction.execute(&format!("DELETE FROM {table} WHERE score < 1"), [])?;
    }
    Ok(())
}

impl Db {
    pub fn record_visit(
        &self,
        location: &str,
        is_folder: bool,
        now_ms: i64,
    ) -> Result<(), DbError> {
        let transaction = self.connection().unchecked_transaction()?;
        decay(&transaction, "frecency")?;
        transaction.execute(
            "INSERT INTO frecency (location, score, last_at, is_folder) VALUES (?1, 1.0, ?2, ?3)
             ON CONFLICT (location) DO UPDATE SET score = score + 1.0, last_at = excluded.last_at,
                 is_folder = excluded.is_folder",
            params![location, now_ms, is_folder],
        )?;
        transaction.execute(
            "INSERT INTO history (location, visited_at) VALUES (?1, ?2)",
            params![location, now_ms],
        )?;
        transaction.execute(
            "DELETE FROM history WHERE id <=
             (SELECT id FROM history ORDER BY id DESC LIMIT 1 OFFSET ?1)",
            [HISTORY_MAX],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn frecent(&self, limit: usize, now_ms: i64) -> Result<Vec<Frecent>, DbError> {
        let mut statement = self.connection().prepare(&format!(
            "SELECT location, is_folder, {WEIGHT} AS weight FROM frecency
             ORDER BY weight DESC, last_at DESC, location LIMIT ?5"
        ))?;
        let rows = statement.query_map(
            params![now_ms, HOUR_MS, DAY_MS, WEEK_MS, limit_of(limit)],
            |row| {
                Ok(Frecent {
                    location: row.get(0)?,
                    is_folder: row.get(1)?,
                    weight: row.get(2)?,
                })
            },
        )?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn record_command(&self, name: &str, now_ms: i64) -> Result<(), DbError> {
        let transaction = self.connection().unchecked_transaction()?;
        decay(&transaction, "command_use")?;
        transaction.execute(
            "INSERT INTO command_use (name, score, last_at) VALUES (?1, 1.0, ?2)
             ON CONFLICT (name) DO UPDATE SET score = score + 1.0, last_at = excluded.last_at",
            params![name, now_ms],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn command_use(&self, limit: usize, now_ms: i64) -> Result<Vec<(String, f64)>, DbError> {
        let mut statement = self.connection().prepare(&format!(
            "SELECT name, {WEIGHT} AS weight FROM command_use
             ORDER BY weight DESC, last_at DESC, name LIMIT ?5"
        ))?;
        let rows = statement.query_map(
            params![now_ms, HOUR_MS, DAY_MS, WEEK_MS, limit_of(limit)],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn recent_commands(&self, limit: usize) -> Result<Vec<String>, DbError> {
        let mut statement = self
            .connection()
            .prepare("SELECT name FROM command_use ORDER BY last_at DESC, name LIMIT ?1")?;
        let rows = statement.query_map([limit_of(limit)], |row| row.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn clear_search_history(&self) -> Result<(), DbError> {
        let transaction = self.connection().unchecked_transaction()?;
        transaction.execute_batch(
            "DELETE FROM history;
             DELETE FROM frecency;
             DELETE FROM command_use;",
        )?;
        transaction.commit()?;
        Ok(())
    }
}
