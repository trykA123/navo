use std::fs;
use std::path::PathBuf;

use rusty_store::{Density as StoredDensity, HiddenFiles, SettingsStore, ThemeMode};
use rusty_ui::app_menu::{Install, QuickChange, QuickSettings};
use rusty_ui::tokens::{Density, ThemeChoice};

use super::hooks;

const OLD_EDITOR_FILE: &str = "[files]\nshow-hidden = false\n\n[editor]\npath = \"code\"\n";

fn change_theme(theme: ThemeChoice) -> QuickChange {
    QuickChange {
        theme: Some(theme),
        ..QuickChange::default()
    }
}

#[test]
fn defaults_show_hidden_files_and_follow_the_system() {
    let dir = tempfile::tempdir().unwrap();
    let menu = hooks(Some(&dir.path().join("ferrite.toml")), None);
    assert_eq!(
        menu.settings,
        QuickSettings {
            theme: ThemeChoice::System,
            density: Density::Normal,
            show_hidden: true,
        }
    );
    assert!(menu.load_error.is_none());
}

#[test]
fn an_old_editor_written_file_keeps_hidden_files_shown_and_stays_valid() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.toml");
    fs::write(&file, OLD_EDITOR_FILE).unwrap();
    let menu = hooks(Some(&file), None);
    assert!(menu.settings.show_hidden);
    assert!(menu.load_error.is_none());
    let (_, report) = SettingsStore::load(&file);
    assert!(report.error.is_none());
}

#[test]
fn each_choice_reaches_the_file_and_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.toml");
    let menu = hooks(Some(&file), None);
    let change = QuickChange {
        theme: Some(ThemeChoice::Dark),
        density: Some(Density::Compact),
        show_hidden: Some(false),
    };
    let stored = (menu.save)(&change).unwrap();
    assert_eq!(stored, change.applied_to(menu.settings));
    let (store, _) = SettingsStore::load(&file);
    let saved = store.current();
    assert_eq!(saved.appearance.mode, ThemeMode::Dark);
    assert_eq!(saved.appearance.density, StoredDensity::Compact);
    assert_eq!(saved.files.hidden_files, HiddenFiles::Hidden);
    assert_eq!(hooks(Some(&file), None).settings, stored);
}

#[test]
fn a_save_writes_only_its_setting_and_keeps_hand_edits() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.toml");
    fs::write(&file, OLD_EDITOR_FILE).unwrap();
    let menu = hooks(Some(&file), None);
    fs::write(
        &file,
        "[appearance]\ndensity = \"comfortable\"\n\n[files]\nhidden-files = \"hidden\"\n",
    )
    .unwrap();
    let stored = (menu.save)(&change_theme(ThemeChoice::Light)).unwrap();
    assert_eq!(stored.theme, ThemeChoice::Light);
    assert_eq!(stored.density, Density::Comfortable);
    assert!(!stored.show_hidden);
    let (store, _) = SettingsStore::load(&file);
    assert_eq!(store.current().appearance.mode, ThemeMode::Light);
    assert_eq!(
        store.current().appearance.density,
        StoredDensity::Comfortable
    );
    assert_eq!(store.current().files.hidden_files, HiddenFiles::Hidden);
}

#[test]
fn an_invalid_file_is_reported_and_left_byte_identical_by_a_save() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.toml");
    let broken = "[appearance]\nfont-size = \"big\"\n";
    fs::write(&file, broken).unwrap();
    let menu = hooks(Some(&file), None);
    let reported = menu.load_error.clone().unwrap();
    assert!(reported.contains("ferrite.toml"), "{reported}");
    assert!(reported.contains("line 2"), "{reported}");
    let failed = (menu.save)(&change_theme(ThemeChoice::Dark)).unwrap_err();
    assert!(failed.contains("ferrite.toml"), "{failed}");
    assert_eq!(fs::read_to_string(&file).unwrap(), broken);
}

#[test]
fn without_a_settings_file_saves_go_nowhere() {
    let menu = hooks(None, None);
    assert!(menu.settings.show_hidden);
    let stored = (menu.save)(&change_theme(ThemeChoice::Dark)).unwrap();
    assert_eq!(stored.theme, ThemeChoice::Dark);
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
