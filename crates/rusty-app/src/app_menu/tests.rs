use std::fs;
use std::path::PathBuf;

use rusty_store::{SettingsStore, ThemeMode};
use rusty_ui::app_menu::Install;
use rusty_ui::tokens::ThemeChoice;

use super::hooks;

#[test]
fn a_saved_theme_reaches_the_settings_file_and_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.toml");
    let first = hooks(Some(&file), None);
    assert_eq!(first.theme, ThemeChoice::System);
    (first.save_theme)(ThemeChoice::Dark);
    let (store, _) = SettingsStore::load(&file);
    assert_eq!(store.current().appearance.mode, ThemeMode::Dark);
    assert_eq!(hooks(Some(&file), None).theme, ThemeChoice::Dark);
}

#[test]
fn saving_the_theme_keeps_other_settings() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.toml");
    fs::write(&file, "[editor]\npath = \"code\"\n").unwrap();
    (hooks(Some(&file), None).save_theme)(ThemeChoice::Light);
    let (store, _) = SettingsStore::load(&file);
    assert_eq!(store.current().appearance.mode, ThemeMode::Light);
    assert_eq!(store.current().editor.path.as_deref(), Some("code"));
}

#[test]
fn without_a_settings_file_the_theme_follows_the_system_and_saves_nowhere() {
    let menu = hooks(None, None);
    assert_eq!(menu.theme, ThemeChoice::System);
    (menu.save_theme)(ThemeChoice::Dark);
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
