use std::fs;
use std::path::PathBuf;

use rusty_store::{Density as StoredDensity, SettingsStore, ThemeMode};
use rusty_ui::app_menu::{Install, QuickSettings};
use rusty_ui::tokens::{Density, ThemeChoice};

use super::hooks;

#[test]
fn saved_choices_reach_the_settings_file_and_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.toml");
    let first = hooks(Some(&file), None);
    assert_eq!(
        first.settings,
        QuickSettings {
            theme: ThemeChoice::System,
            density: Density::Normal,
            show_hidden: true,
        }
    );
    let chosen = QuickSettings {
        theme: ThemeChoice::Dark,
        density: Density::Compact,
        show_hidden: false,
    };
    (first.save)(&chosen);
    let (store, _) = SettingsStore::load(&file);
    let saved = store.current();
    assert_eq!(saved.appearance.mode, ThemeMode::Dark);
    assert_eq!(saved.appearance.density, StoredDensity::Compact);
    assert!(!saved.files.show_hidden);
    assert_eq!(hooks(Some(&file), None).settings, chosen);
}

#[test]
fn saving_keeps_other_settings() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.toml");
    fs::write(&file, "[editor]\npath = \"code\"\n").unwrap();
    let menu = hooks(Some(&file), None);
    (menu.save)(&QuickSettings {
        theme: ThemeChoice::Light,
        ..menu.settings
    });
    let (store, _) = SettingsStore::load(&file);
    assert_eq!(store.current().appearance.mode, ThemeMode::Light);
    assert_eq!(store.current().editor.path.as_deref(), Some("code"));
}

#[test]
fn without_a_settings_file_the_defaults_apply_and_save_nowhere() {
    let menu = hooks(None, None);
    assert_eq!(menu.settings.theme, ThemeChoice::System);
    assert!(menu.settings.show_hidden);
    (menu.save)(&menu.settings);
}

#[test]
fn the_identity_names_the_version_and_the_exe_folder() {
    let exe = PathBuf::from("apps").join("Ferrite").join("ferrite.exe");
    let menu = hooks(None, Some(exe));
    assert_eq!(menu.identity.version.as_ref(), env!("CARGO_PKG_VERSION"));
    assert_eq!(menu.identity.install, Install::Installed);
    let folder = PathBuf::from("apps").join("Ferrite");
    assert_eq!(menu.identity.folder.as_ref(), folder.to_string_lossy());
}
