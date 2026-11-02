use std::collections::BTreeMap;

use rusqlite::params;

use super::{Db, DbError};

impl Db {
    pub fn settings_defaults(&self) -> Result<BTreeMap<String, String>, DbError> {
        let mut statement = self
            .connection()
            .prepare("SELECT key, value FROM settings_defaults")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn record_settings_defaults(
        &self,
        rows: &[(String, String)],
        now: i64,
    ) -> Result<(), DbError> {
        let transaction = self.connection().unchecked_transaction()?;
        {
            let mut statement = transaction.prepare(
                "INSERT INTO settings_defaults (key, value, written_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value, written_at = excluded.written_at",
            )?;
            for (key, value) in rows {
                statement.execute(params![key, value, now])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }
}
