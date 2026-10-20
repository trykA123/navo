use rusqlite::params;

use super::{Db, DbError};

const LAST_USED: &str = "";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePreset {
    pub name: String,
    pub rules: String,
    pub saved_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RenamePresets {
    pub named: Vec<RenamePreset>,
    pub last_used: Option<RenamePreset>,
}

impl Db {
    pub fn save_preset(&self, name: &str, rules_json: &str, now: i64) -> Result<(), DbError> {
        self.connection().execute(
            "INSERT INTO rename_preset (name, rules, saved_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (name) DO UPDATE SET rules = excluded.rules, saved_at = excluded.saved_at",
            params![name, rules_json, now],
        )?;
        Ok(())
    }

    pub fn presets(&self) -> Result<RenamePresets, DbError> {
        let mut statement = self
            .connection()
            .prepare("SELECT name, rules, saved_at FROM rename_preset ORDER BY name")?;
        let rows = statement.query_map([], |row| {
            Ok(RenamePreset {
                name: row.get(0)?,
                rules: row.get(1)?,
                saved_at: row.get(2)?,
            })
        })?;
        let mut presets = RenamePresets::default();
        for row in rows {
            let preset = row?;
            if preset.name == LAST_USED {
                presets.last_used = Some(preset);
            } else {
                presets.named.push(preset);
            }
        }
        Ok(presets)
    }

    pub fn delete_preset(&self, name: &str) -> Result<(), DbError> {
        self.connection()
            .execute("DELETE FROM rename_preset WHERE name = ?1", [name])?;
        Ok(())
    }
}
