use std::path::PathBuf;

use rusty_store::{Db, SettingsError};

pub(super) struct LedgerFile {
    path: Option<PathBuf>,
    db: Option<Db>,
}

impl LedgerFile {
    pub(super) fn new(path: Option<PathBuf>) -> Self {
        Self { path, db: None }
    }

    pub(super) fn open(&mut self) -> Result<Option<&Db>, SettingsError> {
        let Some(path) = &self.path else {
            return Ok(None);
        };
        if self.db.is_none() {
            let (db, _) = Db::open(path, false).map_err(|error| {
                SettingsError::Ledger(format!("opening {}: {error}", path.display()))
            })?;
            self.db = Some(db);
        }
        Ok(self.db.as_ref())
    }
}
