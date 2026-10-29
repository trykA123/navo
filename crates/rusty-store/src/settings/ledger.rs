use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use crate::{Db, DbError, JobJournal};

pub trait SettingsLedger {
    fn settings_defaults(&self) -> Result<BTreeMap<String, String>, DbError>;
    fn record_settings_defaults(&self, rows: &[(String, String)], now: i64) -> Result<(), DbError>;
}

impl SettingsLedger for Db {
    fn settings_defaults(&self) -> Result<BTreeMap<String, String>, DbError> {
        self.settings_defaults()
    }

    fn record_settings_defaults(&self, rows: &[(String, String)], now: i64) -> Result<(), DbError> {
        self.record_settings_defaults(rows, now)
    }
}

impl SettingsLedger for Mutex<JobJournal> {
    fn settings_defaults(&self) -> Result<BTreeMap<String, String>, DbError> {
        self.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .database()
            .settings_defaults()
    }

    fn record_settings_defaults(&self, rows: &[(String, String)], now: i64) -> Result<(), DbError> {
        self.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .database()
            .record_settings_defaults(rows, now)
    }
}
