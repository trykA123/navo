use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use rusty_core::Location;
use rusty_store::{SettingValue, Settings, SettingsError, SettingsStore, settings_key_line};
use rusty_ui::settings::{Change, Control, FieldSpec, Value, Values};
use rusty_ui::window_view::{FindLine, SaveReport, SettingsHooks};

const FILE: &str = "ferrite.toml";

pub fn hooks(path: Option<&Path>) -> SettingsHooks {
    let loaded = path.map(SettingsStore::load);
    let load_error = loaded
        .as_ref()
        .and_then(|(_, report)| report.error.as_ref())
        .map(describe);
    if let Some(message) = &load_error {
        report(message);
    }
    let values = loaded
        .as_ref()
        .map_or_else(Values::default, |(store, _)| values_of(store.current()));
    let store = loaded.map(|(store, _)| Arc::new(Mutex::new(store)));
    let session = Arc::new(Mutex::new(values.clone()));
    SettingsHooks {
        values,
        load_error: load_error.map(Into::into),
        save: Arc::new(move |changes| {
            let saved = match &store {
                Some(store) => save(store, changes),
                None => SaveReport::saved(remember(&session, changes)),
            };
            if let Some(message) = &saved.error {
                report(message);
            }
            saved
        }),
        file: path.and_then(location_of),
        line_of: line_finder(path.map(Path::to_owned)),
    }
}

fn save(store: &Mutex<SettingsStore>, changes: &[Change]) -> SaveReport {
    let mut store = store.lock().unwrap_or_else(PoisonError::into_inner);
    let mut failed = Vec::new();
    let mut error = None;
    for change in changes {
        if let Err(problem) = store.save(change.key(), stored_value(change)) {
            failed.push(change.field);
            error = Some(describe(&problem));
        }
    }
    SaveReport {
        values: values_of(store.current()),
        failed,
        error,
    }
}

fn remember(session: &Mutex<Values>, changes: &[Change]) -> Values {
    let mut values = session.lock().unwrap_or_else(PoisonError::into_inner);
    *values = changes
        .iter()
        .fold(values.clone(), |all, one| all.with(one));
    values.clone()
}

fn location_of(path: &Path) -> Option<Location> {
    Location::new(path.to_str()?).ok()
}

fn line_finder(path: Option<PathBuf>) -> FindLine {
    Arc::new(move |key| {
        let text = fs::read_to_string(path.as_ref()?).ok()?;
        settings_key_line(&text, key).and_then(|line| u64::try_from(line).ok())
    })
}

fn describe(error: &SettingsError) -> String {
    format!("{FILE}: {error}")
}

#[allow(
    clippy::print_stderr,
    reason = "settings errors go to the diagnostics stream until the GUI log exists"
)]
fn report(message: &str) {
    eprintln!("settings: {message}");
}

fn stored_value(change: &Change) -> Option<SettingValue> {
    match (&change.value, change.field.spec().control) {
        (Value::Flag(flag), _) => Some(SettingValue::Bool(*flag)),
        (Value::Number(number), _) => Some(SettingValue::Integer(*number)),
        (Value::Text(text), _) if text.is_empty() => None,
        (Value::Text(text), _) => Some(SettingValue::Text(text.to_string())),
        (Value::Pick(at), Control::Choices(options, _)) => options
            .get(*at)
            .map(|option| SettingValue::Text(option.toml.to_owned())),
        (Value::Pick(_), _) => None,
    }
}

pub fn values_of(settings: &Settings) -> Values {
    let defaults = Values::default();
    Values::from_fn(|spec| {
        value_of(spec, settings.value_at(spec.key))
            .unwrap_or_else(|| defaults.get(spec.field).clone())
    })
}

fn value_of(spec: &FieldSpec, stored: Option<SettingValue>) -> Option<Value> {
    match (spec.control, stored) {
        (Control::Choices(options, _), Some(SettingValue::Text(name))) => options
            .iter()
            .position(|option| option.toml == name)
            .map(Value::Pick),
        (Control::Toggle, Some(SettingValue::Bool(flag))) => Some(Value::Flag(flag)),
        (Control::Number, Some(SettingValue::Integer(number))) => Some(Value::Number(number)),
        (Control::Text, Some(SettingValue::Text(text))) => Some(Value::Text(text.into())),
        (Control::Text, None) => Some(Value::Text("".into())),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
