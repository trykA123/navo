use std::borrow::Cow;
use std::sync::PoisonError;

use super::{
    Saved, SettingValue, SettingsError, SettingsStore, complete, edit, key_line, parse, read,
};

struct Attempt<'a> {
    base: Option<String>,
    backup: &'a mut bool,
}

impl SettingsStore {
    pub fn save(&mut self, key: &str, value: Option<SettingValue>) -> Result<Saved, SettingsError> {
        let session = self.backup.clone();
        let mut backup = session.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut attempt = Attempt {
            base: read(&self.path)?,
            backup: &mut backup,
        };
        self.save_from(key, value, &mut attempt)
    }

    pub fn save_preserving_edits(
        &mut self,
        key: &str,
        value: Option<SettingValue>,
    ) -> Result<Saved, SettingsError> {
        let session = self.backup.clone();
        let mut backup = session.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut attempt = Attempt {
            base: read(&self.path)?,
            backup: &mut backup,
        };
        let before = edit::key_source(attempt.base.as_deref().unwrap_or_default(), key)?;
        match self.save_from(key, value.clone(), &mut attempt) {
            Err(SettingsError::ChangedOnDisk) => {
                attempt.base = read(&self.path)?;
                let after = edit::key_source(attempt.base.as_deref().unwrap_or_default(), key)?;
                if before != after {
                    return Err(SettingsError::FieldChangedOnDisk);
                }
                self.save_from(key, value, &mut attempt)
            }
            done => done,
        }
    }

    fn save_from(
        &mut self,
        key: &str,
        value: Option<SettingValue>,
        attempt: &mut Attempt<'_>,
    ) -> Result<Saved, SettingsError> {
        let base = attempt.base.as_deref();
        if let Some(text) = base {
            parse(text).map_err(|_| SettingsError::FileInvalid)?;
        }
        let source = match base {
            Some(text) => Cow::Borrowed(text),
            None => Cow::Owned(complete::complete_text("", &self.current)?.text),
        };
        let was_present =
            (base.is_some() || self.generation > 0) && key_line(&source, key).is_some();
        let patched = edit::patch(&source, key, value)?;
        let (settings, _) = parse(&patched)?;
        let durability_error = if base != Some(patched.as_str()) {
            self.write(&patched, base, attempt.backup)?
        } else {
            None
        };
        self.take(settings);
        Ok(Saved {
            was_present,
            durability_error,
        })
    }
}
