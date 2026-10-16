use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use rusty_store::{
    Density as StoredDensity, HiddenFiles, SettingValue, Settings, SettingsError, SettingsStore,
    ThemeMode,
};
use rusty_ui::app_menu::{Identity, Install, QuickChange, QuickSettings};
use rusty_ui::tokens::{Density, ThemeChoice};
use rusty_ui::window_view::AppMenuHooks;

const FILE: &str = "ferrite.toml";

pub fn hooks(settings: Option<&Path>, exe: Option<PathBuf>) -> AppMenuHooks {
    let loaded = settings.map(SettingsStore::load);
    let load_error = loaded
        .as_ref()
        .and_then(|(_, report)| report.error.as_ref())
        .map(describe);
    if let Some(message) = &load_error {
        report(message);
    }
    let initial = loaded.as_ref().map_or_else(
        || quick(&Settings::default()),
        |(store, _)| quick(store.current()),
    );
    let target = match loaded {
        Some((store, _)) => Target::File(Box::new(store)),
        None => Target::Session(initial),
    };
    let target = Arc::new(Mutex::new(target));
    AppMenuHooks {
        identity: identity(exe.as_deref()),
        settings: initial,
        load_error: load_error.map(Into::into),
        save: Arc::new(move |change| {
            let mut target = target.lock().unwrap_or_else(PoisonError::into_inner);
            let saved = target.save(change);
            if let Err(message) = &saved {
                report(message);
            }
            saved
        }),
    }
}

enum Target {
    File(Box<SettingsStore>),
    Session(QuickSettings),
}

impl Target {
    fn save(&mut self, change: &QuickChange) -> Result<QuickSettings, String> {
        match self {
            Self::Session(values) => {
                *values = change.applied_to(*values);
                Ok(*values)
            }
            Self::File(store) => save(store, change).map_err(|error| describe(&error)),
        }
    }
}

fn save(store: &mut SettingsStore, change: &QuickChange) -> Result<QuickSettings, SettingsError> {
    for (key, value) in fields(change) {
        store.save(key, Some(SettingValue::Text(value.to_owned())))?;
    }
    Ok(quick(store.current()))
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

fn identity(exe: Option<&Path>) -> Identity {
    let folder = exe
        .and_then(Path::parent)
        .map(|folder| folder.to_string_lossy().into_owned())
        .unwrap_or_default();
    Identity {
        version: env!("CARGO_PKG_VERSION").into(),
        install: Install::Installed,
        folder: folder.into(),
    }
}

fn quick(settings: &Settings) -> QuickSettings {
    let theme = match settings.appearance.mode {
        ThemeMode::System => ThemeChoice::System,
        ThemeMode::Light => ThemeChoice::Light,
        ThemeMode::Dark => ThemeChoice::Dark,
    };
    let density = match settings.appearance.density {
        StoredDensity::Compact => Density::Compact,
        StoredDensity::Normal => Density::Normal,
        StoredDensity::Comfortable => Density::Comfortable,
    };
    QuickSettings {
        theme,
        density,
        show_hidden: settings.files.hidden_files == HiddenFiles::Shown,
    }
}

fn fields(change: &QuickChange) -> Vec<(&'static str, &'static str)> {
    let theme = change.theme.map(|theme| match theme {
        ThemeChoice::System => "system",
        ThemeChoice::Light => "light",
        ThemeChoice::Dark => "dark",
    });
    let density = change.density.map(|density| match density {
        Density::Compact => "compact",
        Density::Normal => "normal",
        Density::Comfortable => "comfortable",
    });
    let hidden = change
        .show_hidden
        .map(|shown| if shown { "shown" } else { "hidden" });
    [
        ("appearance.mode", theme),
        ("appearance.density", density),
        ("files.hidden-files", hidden),
    ]
    .into_iter()
    .filter_map(|(key, value)| Some((key, value?)))
    .collect()
}

#[cfg(test)]
mod tests;
