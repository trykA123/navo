use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use rusty_core::Location;
use rusty_fs::Watch;
use rusty_store::{SettingValue, Settings, SettingsError, SettingsStore, settings_key_line};
use rusty_ui::settings::{Change, Control, FieldSpec, Value, Values};
use rusty_ui::window_view::{FindLine, SaveReport, SettingsHooks, SettingsNews};

use crate::watch_file::watch_file;

mod reload;
mod start;

pub use reload::Reloader;
pub use start::{Loaded, start_from};
#[cfg(test)]
use start::{follow_defaults, spawn, start, start_in_background, start_sharing};

const FILE: &str = "ferrite.toml";

type Shared = Arc<Mutex<SettingsStore>>;

#[cfg(test)]
#[must_use]
pub fn hooks(loaded: Option<Loaded>) -> SettingsHooks {
    parts(loaded).0
}

pub fn live(loaded: Option<Loaded>) -> (SettingsHooks, Option<Watch>) {
    let (carrier, sender) = SettingsHooks::none().channel();
    let (hooks, watching) = watched(loaded, move |news| {
        let _ = sender.send(news);
    });
    let hooks = SettingsHooks {
        news: carrier.news,
        ..hooks
    };
    (hooks, watching)
}

pub fn watched(
    loaded: Option<Loaded>,
    mut on_news: impl FnMut(SettingsNews) + Send + 'static,
) -> (SettingsHooks, Option<Watch>) {
    let (hooks, shared) = parts(loaded);
    let Some((path, store)) = shared else {
        return (hooks, None);
    };
    let problem = hooks.load_error.as_ref().map(ToString::to_string);
    let mut reloader = Reloader::new(store, problem);
    let watching = watch_file(&path, move || {
        if let Some(news) = reloader.reload() {
            on_news(news);
        }
    });
    if watching.is_none() {
        tracing::warn!(
            "the settings folder is not watched; ferrite.toml changes apply at the next start"
        );
    }
    (hooks, watching)
}

fn parts(loaded: Option<Loaded>) -> (SettingsHooks, Option<(PathBuf, Shared)>) {
    let path = loaded.as_ref().map(|loaded| loaded.path.clone());
    let load_error = loaded
        .as_ref()
        .and_then(|loaded| loaded.report.error.as_ref())
        .map(describe);
    if let Some(message) = &load_error {
        report(message);
    }
    let values = loaded
        .as_ref()
        .map_or_else(Values::default, |loaded| values_of(loaded.store.current()));
    let store = loaded.map(|loaded| Arc::new(Mutex::new(loaded.store)));
    let shared = path.clone().zip(store.clone());
    let session = Arc::new(Mutex::new(values.clone()));
    let hooks = SettingsHooks {
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
        file: path.as_deref().and_then(location_of),
        line_of: line_finder(path),
        news: None,
    };
    (hooks, shared)
}

fn save(store: &Mutex<SettingsStore>, changes: &[Change]) -> SaveReport {
    let mut store = store.lock().unwrap_or_else(PoisonError::into_inner);
    let mut report = SaveReport::saved(Values::default());
    for change in changes {
        let value = stored_value(change);
        match retried(|| store.save(change.key(), value.clone())) {
            Ok(saved) if !saved.was_present => report.was_absent.push(change.field),
            Ok(_) => {}
            Err(problem) => {
                report.failed.push(change.field);
                report.error = Some(describe(&problem));
            }
        }
    }
    report.values = values_of(store.current());
    report.generation = store.generation();
    report
}

fn retried<T>(mut save: impl FnMut() -> Result<T, SettingsError>) -> Result<T, SettingsError> {
    match save() {
        Err(SettingsError::ChangedOnDisk) => save(),
        done => done,
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
        (Value::Pick(_) | Value::Unset, _) => None,
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
mod live_tests;
#[cfg(test)]
mod start_tests;
#[cfg(test)]
mod tests;
