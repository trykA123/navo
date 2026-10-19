use super::{Saved, SettingValue, Settings, SettingsError, SettingsStore, ledger_error, now};
use crate::Db;

fn canonical(value: Option<SettingValue>) -> Option<String> {
    let value = match value? {
        SettingValue::Bool(flag) => toml::Value::Boolean(flag),
        SettingValue::Integer(number) => toml::Value::Integer(number),
        SettingValue::Text(text) => toml::Value::String(text),
    };
    Some(value.to_string())
}

impl SettingsStore {
    #[must_use]
    pub fn is_users(&self, key: &str, defaults: &Settings) -> bool {
        let value = canonical(self.current.value_at(key));
        if value == canonical(defaults.value_at(key)) {
            return false;
        }
        let followed = self.followed.get(key).filter(|value| !value.is_empty());
        followed.is_none_or(|followed| value.as_ref() != Some(followed))
    }

    pub fn reset(
        &mut self,
        key: &str,
        ledger: Option<&Db>,
        defaults: &Settings,
    ) -> Result<Saved, SettingsError> {
        let value = defaults.value_at(key);
        let mut saved = self.save_preserving_edits(key, value.clone())?;
        let Some(default) = canonical(value) else {
            return Ok(saved);
        };
        self.followed.insert(key.to_owned(), default.clone());
        if let Some(ledger) = ledger {
            saved.ledger_error = ledger
                .record_settings_defaults(&[(key.to_owned(), default)], now())
                .map_err(ledger_error)
                .err();
        }
        Ok(saved)
    }
}
