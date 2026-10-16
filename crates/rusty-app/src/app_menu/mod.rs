use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use rusty_store::{
    Density as StoredDensity, HiddenFiles, Settings, SettingsError, SettingsStore, ThemeMode,
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
    let mut settings = store.current().clone();
    store_into(&mut settings, change);
    match store.save(settings) {
        Err(SettingsError::FileChanged) => {
            if let Some(error) = store.reload().error {
                return Err(error);
            }
            let mut settings = store.current().clone();
            store_into(&mut settings, change);
            store.save(settings)?;
        }
        other => other?,
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

fn store_into(settings: &mut Settings, change: &QuickChange) {
    if let Some(theme) = change.theme {
        settings.appearance.mode = match theme {
            ThemeChoice::System => ThemeMode::System,
            ThemeChoice::Light => ThemeMode::Light,
            ThemeChoice::Dark => ThemeMode::Dark,
        };
    }
    if let Some(density) = change.density {
        settings.appearance.density = match density {
            Density::Compact => StoredDensity::Compact,
            Density::Normal => StoredDensity::Normal,
            Density::Comfortable => StoredDensity::Comfortable,
        };
    }
    if let Some(shown) = change.show_hidden {
        settings.files.hidden_files = if shown {
            HiddenFiles::Shown
        } else {
            HiddenFiles::Hidden
        };
    }
}

#[cfg(test)]
mod tests;
