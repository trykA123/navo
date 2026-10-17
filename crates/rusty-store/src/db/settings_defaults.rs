use std::collections::BTreeMap;

use super::{Db, DbError};

impl Db {
    pub fn settings_defaults(&self) -> Result<BTreeMap<String, String>, DbError> {
        todo!()
    }

    pub fn record_settings_defaults(
        &self,
        rows: &[(String, String)],
        now: i64,
    ) -> Result<(), DbError> {
        let _ = (rows, now);
        todo!()
    }
}
