use rusty_cache::DiskListings;
use rusty_diag::{DiskKind, Redactor};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::{fs, io};

use rusty_core::Location;
use rusty_fs::Watch;
use rusty_store::{
    Saved, SettingValue, Settings, SettingsError, SettingsStore, settings_key_line,
    settings_section_line,
};
use rusty_ui::settings::{Change, Control, FieldSpec, Value, Values};
use rusty_ui::window_view::{FindLine, KeyLine, SaveReport, SettingsHooks, SettingsNews};

use crate::search_hooks::Remember;
use crate::watch_file::watch_file;
use ledger::LedgerFile;

mod layout_edit;
mod ledger;
mod paused;
mod reload;
mod start;

pub use reload::Reloader;
pub use start::{Loaded, start_from};
#[cfg(test)]
use start::{follow_defaults, spawn, start, start_sharing, start_waiting};

const FILE: &str = "navo.toml";

type Shared = Arc<Mutex<SettingsStore>>;

#[cfg(test)]
#[must_use]
pub fn hooks(loaded: Option<Loaded>) -> SettingsHooks {
    parts(loaded).0
}

pub fn live(loaded: Option<Loaded>, remember: &Remember) -> (SettingsHooks, Option<Watch>) {
    let (carrier, sender) = SettingsHooks::none().channel();
    if let Some(loaded) = &loaded {
        remember.set(loaded.store.current().search.remember);
    }
    let (hooks, watching) = watching(loaded, Some(remember.clone()), move |news| {
        let _ = sender.send(news);
    });
    let hooks = SettingsHooks {
        news: carrier.news,
        ..hooks
    };
    (hooks, watching)
}

#[cfg(test)]
pub fn watched(
    loaded: Option<Loaded>,
    on_news: impl FnMut(SettingsNews) + Send + 'static,
) -> (SettingsHooks, Option<Watch>) {
    watching(loaded, None, on_news)
}

fn watching(
    mut loaded: Option<Loaded>,
    remember: Option<Remember>,
    on_news: impl FnMut(SettingsNews) + Send + 'static,
) -> (SettingsHooks, Option<Watch>) {
    let late = loaded.as_mut().and_then(|loaded| loaded.late.take());
    let changes = loaded
        .as_mut()
        .map(|loaded| std::mem::take(&mut loaded.default_changes))
        .unwrap_or_default();
    let guarded = paused::guarded(loaded.as_ref());
    let (hooks, shared) = parts(loaded);
    let Some((path, store)) = shared else {
        return (hooks, None);
    };
    let problem = hooks.load_error.as_ref().map(ToString::to_string);
    let reloader = Reloader::new(store, problem)
        .remembering(remember)
        .guarding(guarded);
    let reloader = reload::Delivery::new(reloader, on_news);
    let watched = reloader.clone();
    let watching = watch_file(&path, move || {
        reload::deliver(&watched);
    });
    reload::deliver(&reloader);
    reload::announce(&reloader, &changes);
    reload::receive_late(late, reloader);
    if watching.is_none() {
        tracing::warn!(
            "the settings folder is not watched; navo.toml changes apply at the next start"
        );
    }
    (hooks, watching)
}

fn parts(loaded: Option<Loaded>) -> (SettingsHooks, Option<(PathBuf, Shared)>) {
    let path = loaded.as_ref().map(|loaded| loaded.path.clone());
    let guarded = paused::guarded(loaded.as_ref());
    let ledger = Mutex::new(LedgerFile::new(
        loaded.as_ref().and_then(|loaded| loaded.ledger.clone()),
    ));
    let load_error = loaded
        .as_ref()
        .and_then(|loaded| loaded.report.error.as_ref())
        .map(|error| paused::describe(error, guarded.as_deref()));
    if let Some(message) = &load_error {
        report(message);
    }
    let values = loaded
        .as_ref()
        .map_or_else(Values::default, |loaded| values_of(&loaded.store));
    let store = loaded.map(|loaded| Arc::new(Mutex::new(loaded.store)));
    let shared = path.clone().zip(store.clone());
    let edit_layout = layout_edit::hook(store.clone());
    let session = Arc::new(Mutex::new(values.clone()));
    let hooks = SettingsHooks {
        values,
        load_error: load_error.map(Into::into),
        save: Arc::new(move |changes| {
            let saved = match &store {
                Some(store) => save(store, &ledger, changes),
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
        edit_layout,
        ..SettingsHooks::none()
    };
    (hooks, shared)
}

fn save(
    store: &Mutex<SettingsStore>,
    ledger: &Mutex<LedgerFile>,
    changes: &[Change],
) -> SaveReport {
    let mut store = store.lock().unwrap_or_else(PoisonError::into_inner);
    let mut ledger = ledger.lock().unwrap_or_else(PoisonError::into_inner);
    let mut report = SaveReport::saved(Values::default());
    for change in changes {
        match save_one(&mut store, &mut ledger, change) {
            Ok(saved) => {
                if !saved.was_present {
                    report.was_absent.push(change.field);
                }
                let problem = saved.durability_error.or(saved.ledger_error);
                if let Some(error) = problem {
                    report.error = Some(describe(&error));
                }
            }
            Err(problem) => {
                report.failed.push(change.field);
                report.error = Some(describe(&problem));
            }
        }
    }
    report.values = values_of(&store);
    report.generation = store.generation();
    report
}

fn save_one(
    store: &mut SettingsStore,
    ledger: &mut LedgerFile,
    change: &Change,
) -> Result<Saved, SettingsError> {
    if change.value != Value::Reset {
        return store.save_preserving_edits(change.key(), stored_value(change));
    }
    let defaults = Settings::default();
    match ledger.open() {
        Ok(db) => store.reset(change.key(), db, &defaults),
        Err(error) => {
            let mut saved = store.reset(change.key(), None, &defaults)?;
            saved.ledger_error = Some(error);
            Ok(saved)
        }
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
        let found = settings_key_line(&text, key);
        let line = found.unwrap_or_else(|| settings_section_line(&text, key));
        Some(KeyLine {
            line: u64::try_from(line).ok()?,
            is_missing: found.is_none(),
        })
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
        (Value::Text(text), _) if change.field.is_list() => {
            Some(SettingValue::List(split_list(text)))
        }
        (Value::Text(text), _) if text.is_empty() => None,
        (Value::Text(text), _) => Some(SettingValue::Text(text.to_string())),
        (Value::Pick(at), Control::Choices(options, _)) => options
            .get(*at)
            .map(|option| SettingValue::Text(option.toml.to_owned())),
        (Value::Pick(_) | Value::Unset | Value::Reset, _) => None,
    }
}

fn split_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn values_of(store: &SettingsStore) -> Values {
    let defaults = Values::default();
    let settings = store.current();
    let built_in = Settings::default();
    Values::from_fn(|spec| {
        value_of(spec, settings.value_at(spec.key))
            .unwrap_or_else(|| defaults.get(spec.field).clone())
    })
    .marked(|spec| store.is_users(spec.key, &built_in))
    .with_layout_presets(
        settings
            .layout
            .presets
            .iter()
            .map(|preset| (preset.name.as_str(), preset.tiles.as_str())),
    )
}

fn value_of(spec: &FieldSpec, stored: Option<SettingValue>) -> Option<Value> {
    match (spec.control, stored) {
        (Control::Choices(options, _), Some(SettingValue::Text(name))) => options
            .iter()
            .position(|option| option.toml == name)
            .map(Value::Pick),
        (Control::Toggle, Some(SettingValue::Bool(flag))) => Some(Value::Flag(flag)),
        (Control::Number, Some(SettingValue::Integer(number))) => Some(Value::Number(number)),
        (Control::Text, Some(SettingValue::List(items))) => {
            Some(Value::Text(items.join(", ").into()))
        }
        (Control::Text, Some(SettingValue::Text(text))) => Some(Value::Text(text.into())),
        (Control::Text, None) => Some(Value::Text("".into())),
        _ => None,
    }
}

#[cfg(test)]
mod layout_edit_tests;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod notice_tests;
#[cfg(test)]
mod paused_tests;
#[cfg(test)]
mod start_tests;
#[cfg(test)]
mod state_tests;
#[cfg(test)]
mod tests;

pub(crate) fn wire_diagnostics(
    hooks: &mut rusty_ui::window_view::SettingsHooks,
    crash: &crate::crash_report::CrashStart,
    dirs: Option<&rusty_store::StoreDirs>,
) {
    let started = crash.started;
    let diagnostics = &crash.diagnostics;
    let data = dirs.map(|dirs| dirs.data.clone());
    let scan_data = data.clone();
    let scan_diagnostics = diagnostics.clone();
    hooks.disk_use =
        Arc::new(move || diagnostic_rows(scan_data.as_deref(), &scan_diagnostics.redactor));
    let diagnostics = diagnostics.clone();
    hooks.save_diagnostic_report = Arc::new(move || {
        let folder =
            crate::crash::desktop_dir().ok_or_else(|| rusty_l10n::tr("diagnostics-no-desktop"))?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        save_on_desktop(&folder, &stamp, |path| {
            crate::diag::write_on_demand(path, started, &diagnostics, data.as_deref()).map(drop)
        })
        .map_err(|error| error.to_string())
    });
}

fn diagnostic_rows(
    data: Option<&Path>,
    redactor: &Redactor,
) -> Vec<rusty_ui::settings::DiagnosticRow> {
    let mut stats = data.and_then(|data| DiskListings::stored(&data.join("cache.db")));
    crate::diag::disk_use(data, redactor)
        .locations
        .into_iter()
        .map(|location| {
            let id = match location.kind {
                DiskKind::ListingCache => "diagnostics-listings",
                DiskKind::Journal => "diagnostics-journal",
                DiskKind::Session => "diagnostics-session",
                DiskKind::Index => "diagnostics-index",
                DiskKind::LogsAndCrashes => "diagnostics-logs",
                DiskKind::PerformanceRecordings => "diagnostics-recordings",
            };
            let stats = matches!(location.kind, DiskKind::ListingCache)
                .then(|| stats.take())
                .flatten();
            rusty_ui::settings::DiagnosticRow {
                name: rusty_l10n::tr(id).into(),
                path: location.path.into(),
                bytes: location.usage.bytes,
                at_least: location.usage.at_least,
                entries: stats.map(|stats| stats.entries),
                budget: stats.map(|stats| stats.budget),
            }
        })
        .collect()
}

fn save_on_desktop(
    folder: &Path,
    stamp: &str,
    write: impl Fn(&Path) -> io::Result<()>,
) -> io::Result<PathBuf> {
    let mut last = io::Error::from(io::ErrorKind::AlreadyExists);
    for attempt in 1..=9 {
        let suffix = if attempt == 1 {
            String::new()
        } else {
            format!("-{attempt}")
        };
        let path = folder.join(format!(
            "{}-diagnostics-{stamp}{suffix}.zip",
            rusty_core::brand::SLUG
        ));
        match write(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => last = error,
            Err(error) => return Err(error),
        }
    }
    Err(last)
}
