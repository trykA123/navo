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
fn save_creates_a_missing_file_with_only_the_changed_key() {
    let dir = tmp();
    let path = settings_file(&dir);
    let (mut store, _) = SettingsStore::load(&path);
    store.save(MODE, Some(text("dark"))).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "[appearance]\nmode = \"dark\"\n"
    );
}

#[test]
fn save_updates_current() {
    let dir = tmp();
    let (mut store, _) = SettingsStore::load(&settings_file(&dir));
    store.save(MODE, Some(text("dark"))).unwrap();
    assert_eq!(store.current(), &dark_mode());
}

#[test]
fn save_keeps_comments() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(
        &path,
        "# mine\n[appearance]\n# the look\nmode = \"light\" # bright\n",
    );
    let (mut store, _) = SettingsStore::load(&path);
    store.save(MODE, Some(text("dark"))).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "# mine\n[appearance]\n# the look\nmode = \"dark\" # bright\n"
    );
}

#[test]
fn save_keeps_unknown_keys() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[appearance]\nzoom = 2\n\n[plugins]\ngit = true\n");
    let (mut store, _) = SettingsStore::load(&path);
    store.save(MODE, Some(text("dark"))).unwrap();
    let after = fs::read_to_string(&path).unwrap();
    assert!(after.contains("zoom = 2\n"), "{after}");
    assert!(after.contains("[plugins]\ngit = true\n"), "{after}");
    let (_, report) = SettingsStore::load(&path);
    assert_eq!(report.unknown_keys, ["appearance.zoom", "plugins"]);
}

#[test]
fn save_keeps_the_key_order() {
    let dir = tmp();
    let path = settings_file(&dir);
    let before = "[files]\nsort = \"size\"\nfolders-first = false\n\n[appearance]\nfont-size = 15\nmode = \"light\"\ndensity = \"compact\"\n";
    write(&path, before);
    let (mut store, _) = SettingsStore::load(&path);
    store.save(MODE, Some(text("dark"))).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        before.replace("\"light\"", "\"dark\"")
    );
}

#[test]
fn save_adds_a_new_key_to_its_existing_table() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[files]\nsort = \"size\"\n");
    let (mut store, _) = SettingsStore::load(&path);
    store
        .save("files.folders-first", Some(SettingValue::Bool(false)))
        .unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "[files]\nsort = \"size\"\nfolders-first = false\n"
    );
}

#[test]
fn save_without_a_value_removes_the_key() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[editor]\npath = \"code\"\n# keep\n");
    let (mut store, _) = SettingsStore::load(&path);
    store.save("editor.path", None).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "[editor]\n# keep\n");
    assert_eq!(store.current().editor.path, None);
}

#[test]
fn undo_writes_the_old_value_back_and_restores_the_file() {
    let dir = tmp();
    let path = settings_file(&dir);
    let before = "# mine\n[appearance]\nmode = \"light\"\nfont-size = 15\n";
    write(&path, before);
    let (mut store, _) = SettingsStore::load(&path);
    let old = store.current().appearance.mode;
    store.save(MODE, Some(text("dark"))).unwrap();
    store.save(MODE, Some(text("light"))).unwrap();
    assert_eq!(store.current().appearance.mode, old);
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
}

#[test]
fn save_keeps_a_backup_of_the_old_file() {
    let dir = tmp();
    let path = settings_file(&dir);
    let old = "# mine\n[files]\nshow-hidden = true\n";
    write(&path, old);
    let (mut store, _) = SettingsStore::load(&path);
    store.save(MODE, Some(text("dark"))).unwrap();
    assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), old);
}

#[test]
fn save_refuses_when_the_file_on_disk_is_invalid() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[appearance]\nfont-size = \"big\"\n");
    let (mut store, _) = SettingsStore::load(&path);
    assert_eq!(
        store.save(MODE, Some(text("dark"))),
        Err(SettingsError::FileInvalid)
    );
}

#[test]
fn save_refuses_a_file_that_is_not_toml() {
    let dir = tmp();
    let path = settings_file(&dir);
    let text_on_disk = "[appearance\nmode = \"dark\"\n";
    write(&path, text_on_disk);
    let (mut store, _) = SettingsStore::load(&path);
    assert_eq!(
        store.save(MODE, Some(text("light"))),
        Err(SettingsError::FileInvalid)
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), text_on_disk);
}

#[test]
fn refused_save_leaves_the_invalid_file_untouched() {
    let dir = tmp();
    let path = settings_file(&dir);
    let text_on_disk = "[appearance]\nfont-size = \"big\"\n";
    write(&path, text_on_disk);
    let (mut store, _) = SettingsStore::load(&path);
    let _ = store.save(MODE, Some(text("dark")));
    assert_eq!(fs::read(&path).unwrap(), text_on_disk.as_bytes());
    assert!(!backup_path(&path).exists());
}

#[test]
fn refused_save_keeps_current() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "");
    let (mut store, _) = SettingsStore::load(&path);
    write(&path, "[appearance]\nfont-size = \"big\"\n");
    let _ = store.save(MODE, Some(text("dark")));
    assert_eq!(store.current(), &Settings::default());
}

#[test]
fn save_refuses_a_value_of_the_wrong_type_and_writes_nothing() {
    let dir = tmp();
    let path = settings_file(&dir);
    let text_on_disk = "[appearance]\nfont-size = 15\n";
    write(&path, text_on_disk);
    let (mut store, _) = SettingsStore::load(&path);
    let refused = store.save("appearance.font-size", Some(text("big")));
    assert!(
        matches!(refused, Err(SettingsError::Invalid { .. })),
        "{refused:?}"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), text_on_disk);
}

#[test]
fn save_then_load_returns_equal_settings() {
    let dir = tmp();
    let path = settings_file(&dir);
    let (mut store, _) = SettingsStore::load(&path);
    store.save(MODE, Some(text("dark"))).unwrap();
    store
        .save("terminal.command", Some(text("wezterm start --cwd {path}")))
        .unwrap();
    store
        .save("appearance.font-size", Some(SettingValue::Integer(15)))
        .unwrap();
    let (loaded, report) = SettingsStore::load(&path);
    assert_eq!(loaded.current(), store.current());
    assert_eq!(loaded.current().appearance.font_size, 15);
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
        store.save(MODE, Some(text("dark"))),
        Err(SettingsError::Io { .. })
    ));
}

#[test]
fn save_after_an_outside_edit_keeps_the_edit() {
    let dir = tmp();
    let path = settings_file(&dir);
    let (mut store, _) = SettingsStore::load(&path);
    write(&path, "# my comment\n[appearance]\nfont-size = 15\n");
    store.save(MODE, Some(text("dark"))).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "# my comment\n[appearance]\nfont-size = 15\nmode = \"dark\"\n"
    );
    assert_eq!(store.current().appearance.font_size, 15);
}

#[test]
fn consecutive_saves_both_land() {
    let dir = tmp();
    let path = settings_file(&dir);
    let (mut store, _) = SettingsStore::load(&path);
    store.save(MODE, Some(text("dark"))).unwrap();
    store
        .save("files.hidden-files", Some(text("hidden")))
        .unwrap();
    let (loaded, _) = SettingsStore::load(&path);
    assert_eq!(loaded.current().appearance.mode, ThemeMode::Dark);
    assert_eq!(loaded.current().files.hidden_files, HiddenFiles::Hidden);
}

#[test]
fn the_line_of_a_key_is_found_in_the_file() {
    let text_on_disk = "# mine\n[files]\nsort = \"size\"\n\n[appearance]\nmode = \"dark\"\n";
    assert_eq!(key_line(text_on_disk, MODE), Some(6));
    assert_eq!(key_line(text_on_disk, "files.sort"), Some(3));
}

#[test]
fn the_line_of_a_dotted_or_inline_key_is_found() {
    let text_on_disk = "appearance.mode = \"dark\"\nfiles = { sort = \"size\" }\n";
    assert_eq!(key_line(text_on_disk, MODE), Some(1));
    assert_eq!(key_line(text_on_disk, "files.sort"), Some(2));
}

#[test]
fn a_missing_key_or_broken_file_has_no_line() {
    assert_eq!(key_line("[files]\nsort = \"size\"\n", MODE), None);
    assert_eq!(key_line("[appearance\n", MODE), None);
}

#[test]
fn an_old_editor_written_file_loads_valid_with_hidden_files_shown() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(
        &path,
        "[files]\nshow-hidden = false\n\n[editor]\npath = \"C:\\\\Tools\\\\code.exe\"\n",
    );
    let (store, report) = SettingsStore::load(&path);
    assert_eq!(report, LoadReport::default());
    assert_eq!(store.current().files.hidden_files, HiddenFiles::Shown);
    assert_eq!(
        store.current().editor.path.as_deref(),
        Some(r"C:\Tools\code.exe")
    );
}
