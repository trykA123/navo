use std::borrow::Cow;
use std::sync::PoisonError;

use super::{
    Saved, SettingValue, SettingsError, SettingsStore, complete, edit::SaveDocument, parse, read,
};

struct Attempt<'a> {
    base: Option<String>,
    backup: &'a mut bool,
    document: SaveDocument,
}

impl SettingsStore {
    #[cfg(test)]
    pub(crate) fn save(
        &mut self,
        key: &str,
        value: Option<SettingValue>,
    ) -> Result<Saved, SettingsError> {
        let session = self.backup.clone();
        let mut backup = session.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut attempt = self.attempt(key, &mut backup)?;
        self.save_from(key, value, &mut attempt)
    }

    pub fn save_preserving_edits(
        &mut self,
        key: &str,
        value: Option<SettingValue>,
    ) -> Result<Saved, SettingsError> {
        let session = self.backup.clone();
        let mut backup = session.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut attempt = self.attempt(key, &mut backup)?;
        let before = attempt.document.key_source.clone();
        match self.save_from(key, value.clone(), &mut attempt) {
            Err(SettingsError::ChangedOnDisk) => {
                attempt = self.attempt(key, &mut backup)?;
                let after = &attempt.document.key_source;
                if &before != after {
                    return Err(SettingsError::FieldChangedOnDisk);
                }
                self.save_from(key, value, &mut attempt)
            }
            done => done,
        }
    }

    pub fn edit_text(
        &mut self,
        mut edit: impl FnMut(&str) -> Result<String, SettingsError>,
    ) -> Result<Saved, SettingsError> {
        let session = self.backup.clone();
        let mut backup = session.0.lock().unwrap_or_else(PoisonError::into_inner);
        match self.edit_once(&mut edit, &mut backup) {
            Err(SettingsError::ChangedOnDisk) => self.edit_once(&mut edit, &mut backup),
            done => done,
        }
    }

    fn edit_once(
        &mut self,
        edit: &mut impl FnMut(&str) -> Result<String, SettingsError>,
        backup: &mut bool,
    ) -> Result<Saved, SettingsError> {
        let base = read(&self.path)?;
        if let Some(text) = &base {
            parse(text).map_err(|_| SettingsError::FileInvalid)?;
        }
        let source = match base.as_deref() {
            Some(text) => Cow::Borrowed(text),
            None => Cow::Owned(complete::complete_text("", &self.current)?.text),
        };
        let edited = edit(&source)?;
        let (settings, _) = parse(&edited)?;
        let durability_error = if base.as_deref() == Some(edited.as_str()) {
            None
        } else {
            self.write(&edited, base.as_deref(), backup)?
        };
        self.take(settings);
        Ok(Saved {
            was_present: true,
            durability_error,
            ledger_error: None,
        })
    }

    fn attempt<'a>(&self, key: &str, backup: &'a mut bool) -> Result<Attempt<'a>, SettingsError> {
        let base = read(&self.path)?;
        let source = match base.as_deref() {
            Some(text) => Cow::Borrowed(text),
            None => Cow::Owned(complete::complete_text("", &self.current)?.text),
        };
        let document = SaveDocument::parse(&source, key)?;
        Ok(Attempt {
            base,
            backup,
            document,
        })
    }

    fn save_from(
        &mut self,
        key: &str,
        value: Option<SettingValue>,
        attempt: &mut Attempt<'_>,
    ) -> Result<Saved, SettingsError> {
        let base = attempt.base.as_deref();
        let was_present =
            (base.is_some() || self.generation > 0) && attempt.document.key_source.is_some();
        attempt.document.patch(key, value)?;
        let settings = attempt.document.settings()?;
        let patched = attempt.document.text();
        let durability_error = if base != Some(patched.as_str()) {
            self.write(&patched, base, attempt.backup)?
        } else {
            None
        };
        self.take(settings);
        Ok(Saved {
            was_present,
            durability_error,
            ledger_error: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_save_parses_the_source_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("navo.toml");
        std::fs::write(
            &path,
            r#"[appearance]
mode = "light"
"#,
        )
        .unwrap();
        let (mut store, _) = SettingsStore::load(&path);
        super::super::edit::SAVE_PARSES.with(|count| count.set(0));
        store
            .save_preserving_edits("appearance.mode", Some(SettingValue::Text("dark".into())))
            .unwrap();
        super::super::edit::SAVE_PARSES.with(|count| assert_eq!(count.get(), 1));
        assert_eq!(
            store.current().appearance.mode,
            super::super::ThemeMode::Dark
        );
    }
}
