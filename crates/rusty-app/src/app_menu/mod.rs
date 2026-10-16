use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rusty_store::{SettingsError, SettingsStore, ThemeMode};
use rusty_ui::app_menu::{Identity, Install};
use rusty_ui::tokens::ThemeChoice;
use rusty_ui::window_view::AppMenuHooks;

pub fn hooks(settings: Option<&Path>, exe: Option<PathBuf>) -> AppMenuHooks {
    let store = settings.map(|file| Rc::new(RefCell::new(SettingsStore::load(file).0)));
    let theme = store.as_ref().map_or(ThemeChoice::System, |store| {
        choice(store.borrow().current().appearance.mode)
    });
    AppMenuHooks {
        identity: identity(exe.as_deref()),
        theme,
        save_theme: Rc::new(move |choice| {
            if let Some(store) = &store {
                save_theme(store, choice);
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

fn choice(mode: ThemeMode) -> ThemeChoice {
    match mode {
        ThemeMode::System => ThemeChoice::System,
        ThemeMode::Light => ThemeChoice::Light,
        ThemeMode::Dark => ThemeChoice::Dark,
    }
}

fn mode(choice: ThemeChoice) -> ThemeMode {
    match choice {
        ThemeChoice::System => ThemeMode::System,
        ThemeChoice::Light => ThemeMode::Light,
        ThemeChoice::Dark => ThemeMode::Dark,
    }
}

fn save_theme(store: &RefCell<SettingsStore>, choice: ThemeChoice) {
    for _ in 0..2 {
        let mut settings = store.borrow().current().clone();
        settings.appearance.mode = mode(choice);
        let saved = store.borrow_mut().save(settings);
        if !matches!(saved, Err(SettingsError::FileChanged)) {
            return;
        }
        let _ = store.borrow_mut().reload();
    }
}

#[cfg(test)]
mod tests;
