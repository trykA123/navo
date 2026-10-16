use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rusty_store::{Density as StoredDensity, Settings, SettingsError, SettingsStore, ThemeMode};
use rusty_ui::app_menu::{Identity, Install, QuickSettings};
use rusty_ui::tokens::{Density, ThemeChoice};
use rusty_ui::window_view::AppMenuHooks;

pub fn hooks(settings: Option<&Path>, exe: Option<PathBuf>) -> AppMenuHooks {
    let store = settings.map(|file| Rc::new(RefCell::new(SettingsStore::load(file).0)));
    let quick = store.as_ref().map_or_else(
        || quick(&Settings::default()),
        |store| quick(store.borrow().current()),
    );
    AppMenuHooks {
        identity: identity(exe.as_deref()),
        settings: quick,
        save: Rc::new(move |chosen| {
            if let Some(store) = &store {
                save(store, chosen);
            }
        }),
    }
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
        show_hidden: settings.files.show_hidden,
    }
}

fn store_into(settings: &mut Settings, chosen: &QuickSettings) {
    settings.appearance.mode = match chosen.theme {
        ThemeChoice::System => ThemeMode::System,
        ThemeChoice::Light => ThemeMode::Light,
        ThemeChoice::Dark => ThemeMode::Dark,
    };
    settings.appearance.density = match chosen.density {
        Density::Compact => StoredDensity::Compact,
        Density::Normal => StoredDensity::Normal,
        Density::Comfortable => StoredDensity::Comfortable,
    };
    settings.files.show_hidden = chosen.show_hidden;
}

fn save(store: &RefCell<SettingsStore>, chosen: &QuickSettings) {
    for _ in 0..2 {
        let mut settings = store.borrow().current().clone();
        store_into(&mut settings, chosen);
        let saved = store.borrow_mut().save(settings);
        if !matches!(saved, Err(SettingsError::FileChanged)) {
            return;
        }
        let _ = store.borrow_mut().reload();
    }
}

#[cfg(test)]
mod tests;
