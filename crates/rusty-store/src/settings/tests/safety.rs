use crate::Db;

use super::*;

const HAND_EDIT: &str = "[appearance]\nmode = \"light\"\nfont-size = 20\n";

fn ledger() -> Db {
    Db::open_in_memory().unwrap()
}

fn sorted_by(sort: SortKey) -> Settings {
    let mut settings = Settings::default();
    settings.files.sort = sort;
    settings
}

fn recorded_sort(db: &Db) -> Option<String> {
    db.settings_defaults().unwrap().get("files.sort").cloned()
}

fn break_ledger_writes(db: &Db) {
    db.connection()
        .execute_batch(
            "CREATE TRIGGER no_insert BEFORE INSERT ON settings_defaults
               BEGIN SELECT RAISE(FAIL, 'ledger is full'); END;
             CREATE TRIGGER no_update BEFORE UPDATE ON settings_defaults
               BEGIN SELECT RAISE(FAIL, 'ledger is full'); END;",
        )
        .unwrap();
}

fn mend_ledger_writes(db: &Db) {
    db.connection()
        .execute_batch("DROP TRIGGER no_insert; DROP TRIGGER no_update;")
        .unwrap();
}

#[test]
fn a_failed_ledger_write_still_reports_the_change_and_the_next_start_adopts_the_key() {
    let dir = tmp();
    let path = settings_file(&dir);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&db, &Settings::default()).unwrap();
    break_ledger_writes(&db);
    let (mut next, _) = SettingsStore::load(&path);
    let done = next.complete(&db, &sorted_by(SortKey::Date)).unwrap();
    assert!(done.written);
    assert_eq!(
        done.changes,
        [DefaultChange {
            key: "files.sort".to_owned(),
            old: "\"name\"".to_owned(),
            new: "\"date\"".to_owned(),
        }]
    );
    assert!(matches!(done.ledger_error, Some(SettingsError::Ledger(_))));
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .contains("sort = \"date\"\n")
    );
    assert_eq!(recorded_sort(&db).as_deref(), Some("\"name\""));
    mend_ledger_writes(&db);
    let (mut third, _) = SettingsStore::load(&path);
    let again = third.complete(&db, &sorted_by(SortKey::Date)).unwrap();
    assert!(!again.written);
    assert_eq!(recorded_sort(&db).as_deref(), Some("\"date\""));
    let (mut fourth, _) = SettingsStore::load(&path);
    let later = fourth.complete(&db, &sorted_by(SortKey::Size)).unwrap();
    assert_eq!(later.changes.len(), 1, "the key still follows new defaults");
    assert_eq!(fourth.current().files.sort, SortKey::Size);
}

#[test]
fn a_crash_between_the_write_and_the_ledger_is_healed_at_the_next_start() {
    let dir = tmp();
    let path = settings_file(&dir);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&db, &Settings::default()).unwrap();
    write(
        &path,
        &fs::read_to_string(&path)
            .unwrap()
            .replace("\"name\"", "\"date\""),
    );
    let (mut next, _) = SettingsStore::load(&path);
    let done = next.complete(&db, &sorted_by(SortKey::Date)).unwrap();
    assert_eq!(done.changes, []);
    assert_eq!(recorded_sort(&db).as_deref(), Some("\"date\""));
}

#[test]
fn a_hand_edit_during_a_sheet_save_is_kept_and_the_save_fails() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[appearance]\nmode = \"light\"\n");
    let (mut store, _) = SettingsStore::load(&path);
    on_next_write(|path| write(path, HAND_EDIT));
    assert_eq!(
        store.save(MODE, Some(text("dark"))),
        Err(SettingsError::ChangedOnDisk)
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), HAND_EDIT);
    assert_eq!(store.current().appearance.mode, ThemeMode::Light);
    let left: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(left, ["ferrite.toml"]);
    store.save(MODE, Some(text("dark"))).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "[appearance]\nmode = \"dark\"\nfont-size = 20\n"
    );
}

#[test]
fn a_hand_edit_during_completion_is_kept_and_nothing_is_recorded() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[appearance]\nmode = \"light\"\n");
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    on_next_write(|path| write(path, HAND_EDIT));
    assert_eq!(
        store.complete(&db, &Settings::default()),
        Err(SettingsError::ChangedOnDisk)
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), HAND_EDIT);
    assert!(db.settings_defaults().unwrap().is_empty());
}

#[test]
fn a_file_created_during_a_save_is_kept() {
    let dir = tmp();
    let path = settings_file(&dir);
    let (mut store, _) = SettingsStore::load(&path);
    on_next_write(|path| write(path, HAND_EDIT));
    assert_eq!(
        store.save(MODE, Some(text("dark"))),
        Err(SettingsError::ChangedOnDisk)
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), HAND_EDIT);
}

#[test]
fn a_sheet_save_after_deletion_restores_all_current_settings() {
    let dir = tmp();
    let path = settings_file(&dir);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&db, &Settings::default()).unwrap();
    let recorded = db.settings_defaults().unwrap();
    let mut expected = sorted_by(SortKey::Size);
    expected.appearance.density = Density::Compact;
    expected.files.hidden_files = HiddenFiles::Hidden;
    expected.selection.patterns_select_folders = true;
    expected.terminal.command = Some("terminal --cwd {path}".to_owned());
    expected.editor.path = Some("editor".to_owned());
    expected.experimental.insert("preview".to_owned(), true);
    write(&path, &to_toml(&expected));
    assert_eq!(store.reload(), LoadReport::default());
    fs::remove_file(&path).unwrap();
    assert_eq!(store.reload(), LoadReport::default());
    let saved = store.save(MODE, Some(text("dark"))).unwrap();
    assert!(!saved.was_present);
    expected.appearance.mode = ThemeMode::Dark;
    let (mut next, report) = SettingsStore::load(&path);
    assert_eq!(report, LoadReport::default());
    assert_eq!(next.current(), &expected);
    assert_eq!(store.current(), &expected);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        complete_text("", &expected).unwrap().text
    );
    assert_eq!(db.settings_defaults().unwrap(), recorded);
    let mut upgraded = sorted_by(SortKey::Date);
    upgraded.appearance.font_size = 16;
    let done = next.complete(&db, &upgraded).unwrap();
    assert!(done.ledger_error.is_none());
    assert_eq!(done.changes.len(), 1);
    assert_eq!(done.changes[0].key, "appearance.font-size");
    expected.appearance.font_size = 16;
    assert_eq!(next.current(), &expected);
}

#[test]
fn a_sheet_save_on_a_mixed_file_keeps_each_line_ending() {
    let dir = tmp();
    let path = settings_file(&dir);
    let before = "# mine\n[appearance]\r\nmode = \"light\"\r\nfont-size = 15\n";
    write(&path, before);
    let (mut store, _) = SettingsStore::load(&path);
    store.save(MODE, Some(text("dark"))).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        before.replace("light", "dark")
    );
}

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn mode_of(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn set_mode(path: &Path, mode: u32) {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn a_read_only_file_is_never_completed_or_saved() {
        let dir = tmp();
        let path = settings_file(&dir);
        let mine = "[appearance]\nmode = \"dark\"\n";
        write(&path, mine);
        set_mode(&path, 0o444);
        let db = ledger();
        let (mut store, _) = SettingsStore::load(&path);
        assert_eq!(
            store.complete(&db, &Settings::default()),
            Err(SettingsError::ReadOnly)
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), mine);
        assert_eq!(mode_of(&path), 0o444);
        assert!(db.settings_defaults().unwrap().is_empty());
        let refused = store.save(MODE, Some(text("light")));
        assert_eq!(refused, Err(SettingsError::ReadOnly));
        assert!(SettingsError::ReadOnly.to_string().contains("read-only"));
        assert_eq!(fs::read_to_string(&path).unwrap(), mine);
        assert_eq!(store.current().appearance.mode, ThemeMode::Dark);
        set_mode(&path, 0o644);
    }

    #[test]
    fn completion_and_saves_keep_the_file_mode() {
        let dir = tmp();
        let path = settings_file(&dir);
        write(&path, "[appearance]\nmode = \"dark\"\n");
        set_mode(&path, 0o600);
        let (mut store, _) = SettingsStore::load(&path);
        let done = store.complete(&ledger(), &Settings::default()).unwrap();
        assert!(done.written);
        assert_eq!(mode_of(&path), 0o600);
        store.save(MODE, Some(text("light"))).unwrap();
        assert_eq!(mode_of(&path), 0o600);
    }
}
