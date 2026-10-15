use super::*;

#[test]
fn load_of_a_missing_file_gives_defaults_and_an_empty_report() {
    let dir = tmp();
    let (store, report) = SettingsStore::load(&settings_file(&dir));
    assert_eq!(store.current(), &Settings::default());
    assert_eq!(report, LoadReport::default());
}

#[test]
fn load_of_a_missing_file_creates_nothing() {
    let dir = tmp();
    let path = settings_file(&dir);
    let _ = SettingsStore::load(&path);
    assert!(!path.exists());
}

#[test]
fn load_of_a_valid_file_uses_it() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[appearance]\nmode = \"dark\"\n");
    let (store, report) = SettingsStore::load(&path);
    assert_eq!(store.current(), &dark_mode());
    assert_eq!(report, LoadReport::default());
}

#[test]
fn load_reports_unknown_keys() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[appearance]\nmode = \"dark\"\nzoom = 2\n");
    let (_, report) = SettingsStore::load(&path);
    assert_eq!(report.unknown_keys, ["appearance.zoom"]);
}

#[test]
fn load_of_an_invalid_file_gives_defaults_and_the_error() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[appearance]\nfont-size = \"big\"\n");
    let (store, report) = SettingsStore::load(&path);
    assert_eq!(store.current(), &Settings::default());
    assert!(
        report.error.as_ref().is_some_and(invalid_at(2, 13)),
        "{report:?}"
    );
}

#[test]
fn load_of_an_invalid_file_leaves_it_untouched() {
    let dir = tmp();
    let path = settings_file(&dir);
    let text = "[appearance]\nfont-size = \"big\"\n";
    write(&path, text);
    let _ = SettingsStore::load(&path);
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
}

#[test]
fn reload_after_a_valid_edit_uses_the_new_file() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "");
    let (mut store, _) = SettingsStore::load(&path);
    write(&path, "[appearance]\nmode = \"dark\"\n");
    let report = store.reload();
    assert_eq!(store.current(), &dark_mode());
    assert_eq!(report, LoadReport::default());
}

#[test]
fn reload_after_an_invalid_edit_keeps_the_previous_settings() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[appearance]\nmode = \"dark\"\n");
    let (mut store, _) = SettingsStore::load(&path);
    write(
        &path,
        "[appearance]\nmode = \"dark\"\nfont-size = \"big\"\n",
    );
    let _ = store.reload();
    assert_eq!(store.current(), &dark_mode());
}

#[test]
fn reload_after_an_invalid_edit_reports_the_error() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "");
    let (mut store, _) = SettingsStore::load(&path);
    write(
        &path,
        "[appearance]\nmode = \"dark\"\nfont-size = \"big\"\n",
    );
    let report = store.reload();
    assert!(
        report.error.as_ref().is_some_and(invalid_at(3, 13)),
        "{report:?}"
    );
}

#[test]
fn save_creates_a_missing_file() {
    let dir = tmp();
    let path = settings_file(&dir);
    let (mut store, _) = SettingsStore::load(&path);
    store.save(dark_mode()).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), to_toml(&dark_mode()));
}

#[test]
fn save_updates_current() {
    let dir = tmp();
    let (mut store, _) = SettingsStore::load(&settings_file(&dir));
    store.save(dark_mode()).unwrap();
    assert_eq!(store.current(), &dark_mode());
}

#[test]
fn save_keeps_a_backup_of_the_old_file() {
    let dir = tmp();
    let path = settings_file(&dir);
    let old = "# mine\n[files]\nshow-hidden = true\n";
    write(&path, old);
    let (mut store, _) = SettingsStore::load(&path);
    store.save(dark_mode()).unwrap();
    assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), old);
}

#[test]
fn save_refuses_when_the_file_on_disk_is_invalid() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[appearance]\nfont-size = \"big\"\n");
    let (mut store, _) = SettingsStore::load(&path);
    assert_eq!(store.save(dark_mode()), Err(SettingsError::FileInvalid));
}

#[test]
fn refused_save_leaves_the_invalid_file_untouched() {
    let dir = tmp();
    let path = settings_file(&dir);
    let text = "[appearance]\nfont-size = \"big\"\n";
    write(&path, text);
    let (mut store, _) = SettingsStore::load(&path);
    let _ = store.save(dark_mode());
    assert_eq!(fs::read(&path).unwrap(), text.as_bytes());
    assert!(!backup_path(&path).exists());
}

#[test]
fn refused_save_keeps_current() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "");
    let (mut store, _) = SettingsStore::load(&path);
    write(&path, "[appearance]\nfont-size = \"big\"\n");
    let _ = store.save(dark_mode());
    assert_eq!(store.current(), &Settings::default());
}

#[test]
fn save_then_load_returns_equal_settings() {
    let dir = tmp();
    let path = settings_file(&dir);
    let mut settings = dark_mode();
    settings.terminal.command = Some("wezterm start --cwd {path}".to_owned());
    settings.experimental.insert("new-grid".to_owned(), true);
    let (mut store, _) = SettingsStore::load(&path);
    store.save(settings.clone()).unwrap();
    let (loaded, report) = SettingsStore::load(&path);
    assert_eq!(loaded.current(), &settings);
    assert_eq!(report, LoadReport::default());
}

#[test]
fn load_of_an_unreadable_path_reports_an_io_error() {
    let dir = tmp();
    let path = settings_file(&dir);
    fs::create_dir(&path).unwrap();
    let (store, report) = SettingsStore::load(&path);
    assert_eq!(store.current(), &Settings::default());
    assert!(
        matches!(report.error, Some(SettingsError::Io { .. })),
        "{report:?}"
    );
}

#[test]
fn save_to_an_unreadable_path_is_an_io_error() {
    let dir = tmp();
    let path = settings_file(&dir);
    fs::create_dir(&path).unwrap();
    let (mut store, _) = SettingsStore::load(&path);
    assert!(matches!(
        store.save(Settings::default()),
        Err(SettingsError::Io { .. })
    ));
}

#[test]
fn save_refuses_a_valid_edit_made_after_the_load() {
    let dir = tmp();
    let path = settings_file(&dir);
    let (mut store, _) = SettingsStore::load(&path);
    let edit = "# my comment\n[appearance]\nfont-size = 15\n";
    write(&path, edit);
    assert_eq!(store.save(dark_mode()), Err(SettingsError::FileChanged));
    assert_eq!(fs::read_to_string(&path).unwrap(), edit);
}

#[test]
fn save_after_reloading_the_edit_succeeds() {
    let dir = tmp();
    let path = settings_file(&dir);
    let (mut store, _) = SettingsStore::load(&path);
    write(&path, "[appearance]\nfont-size = 15\n");
    store.reload();
    assert_eq!(store.save(dark_mode()), Ok(()));
}

#[test]
fn consecutive_saves_do_not_count_as_edits() {
    let dir = tmp();
    let path = settings_file(&dir);
    let (mut store, _) = SettingsStore::load(&path);
    store.save(dark_mode()).unwrap();
    assert_eq!(store.save(Settings::default()), Ok(()));
}
