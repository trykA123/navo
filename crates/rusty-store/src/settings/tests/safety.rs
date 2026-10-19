use crate::Db;
use crate::atomic_write::{Stage, tests::on_stage};

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

fn pending_file(path: &Path) -> PathBuf {
    path.with_extension("toml.defaults-pending")
}

fn crash_completion(store: &mut SettingsStore, db: &Db, defaults: &Settings, stage: Stage) {
    on_stage(move |path, reached| {
        if path.file_name().unwrap() == "ferrite.toml" && reached == stage {
            panic!("injected crash at {stage:?}");
        }
        Ok(())
    });
    let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        store.complete(db, defaults)
    }));
    on_stage(|_, _| Ok(()));
    assert!(stopped.is_err());
}

#[test]
fn a_crash_before_the_rename_records_nothing_and_keeps_keys_following() {
    for present in [false, true] {
        let dir = tmp();
        let path = settings_file(&dir);
        let db = ledger();
        let (mut store, _) = SettingsStore::load(&path);
        if present {
            store.complete(&db, &Settings::default()).unwrap();
        }
        let before = fs::read_to_string(&path).ok();
        let recorded = db.settings_defaults().unwrap();
        crash_completion(
            &mut store,
            &db,
            &sorted_by(SortKey::Date),
            Stage::BeforeRename,
        );
        assert_eq!(fs::read_to_string(&path).ok(), before);
        pending::recover(&path, &db).unwrap();
        assert_eq!(db.settings_defaults().unwrap(), recorded);
        assert!(!pending_file(&path).exists());
        let (mut next, _) = SettingsStore::load(&path);
        next.complete(&db, &sorted_by(SortKey::Size)).unwrap();
        assert_eq!(next.current().files.sort, SortKey::Size);
        assert_eq!(recorded_sort(&db).as_deref(), Some("\"size\""));
    }
}

#[test]
fn a_crash_after_the_rename_before_the_committed_mark_records_the_write() {
    let dir = tmp();
    let path = settings_file(&dir);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&db, &Settings::default()).unwrap();
    crash_completion(&mut store, &db, &sorted_by(SortKey::Date), Stage::Replaced);
    assert_eq!(recorded_sort(&db).as_deref(), Some("\"name\""));
    pending::recover(&path, &db).unwrap();
    assert_eq!(recorded_sort(&db).as_deref(), Some("\"date\""));
    assert!(!pending_file(&path).exists());
    let (mut next, _) = SettingsStore::load(&path);
    next.complete(&db, &sorted_by(SortKey::Size)).unwrap();
    assert_eq!(next.current().files.sort, SortKey::Size);
}

#[test]
fn a_failed_committed_mark_keeps_keys_following_after_a_sheet_save() {
    let dir = tmp();
    let path = settings_file(&dir);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&db, &Settings::default()).unwrap();
    let mut pending_writes = 0;
    on_stage(move |path, stage| {
        if path.file_name().unwrap() == "ferrite.toml.defaults-pending"
            && stage == Stage::TempWritten
        {
            pending_writes += 1;
            if pending_writes == 2 {
                return Err(io::Error::other("injected committed mark failure"));
            }
        }
        Ok(())
    });
    let done = store.complete(&db, &sorted_by(SortKey::Date)).unwrap();
    on_stage(|_, _| Ok(()));
    assert!(matches!(done.ledger_error, Some(SettingsError::Io { .. })));
    store
        .save_preserving_edits(MODE, Some(text("dark")))
        .unwrap();
    let (mut next, _) = SettingsStore::load(&path);
    next.complete(&db, &sorted_by(SortKey::Size)).unwrap();
    assert_eq!(next.current().files.sort, SortKey::Size);
    assert_eq!(recorded_sort(&db).as_deref(), Some("\"size\""));
    assert!(!pending_file(&path).exists());
}

#[test]
fn an_unparsable_pending_file_is_quarantined_and_completion_continues() {
    let dir = tmp();
    let path = settings_file(&dir);
    let pending = pending_file(&path);
    let garbage = "not valid [ pending TOML";
    write(&pending, garbage);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    let done = store.complete(&db, &Settings::default()).unwrap();
    assert!(done.written);
    assert!(matches!(
        done.ledger_error,
        Some(SettingsError::Ledger(ref message)) if message.contains(".defaults-pending.bad")
    ));
    assert!(!pending.exists());
    assert_eq!(
        fs::read_to_string(pending.with_extension("defaults-pending.bad")).unwrap(),
        garbage
    );
    let (mut next, _) = SettingsStore::load(&path);
    let done = next.complete(&db, &sorted_by(SortKey::Date)).unwrap();
    assert!(done.ledger_error.is_none());
    assert_eq!(next.current().files.sort, SortKey::Date);
}

#[test]
fn a_noop_completion_records_defaults_without_writing_a_pending_file() {
    let dir = tmp();
    let path = settings_file(&dir);
    let before = complete_text("", &Settings::default()).unwrap().text;
    write(&path, &before);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    on_stage(|_, _| Err(io::Error::other("unexpected file write")));
    let done = store.complete(&db, &Settings::default());
    on_stage(|_, _| Ok(()));
    assert_eq!(done.unwrap(), Completion::default());
    assert_eq!(db.settings_defaults().unwrap().len(), 25);
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
    assert!(!pending_file(&path).exists());
}

#[test]
fn a_failed_pending_discard_keeps_the_original_write_error() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, HAND_EDIT);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    on_stage(|path, stage| {
        if path.file_name().unwrap() == "ferrite.toml" && stage == Stage::TempWritten {
            let pending = pending_file(path);
            fs::remove_file(&pending).unwrap();
            fs::create_dir(&pending).unwrap();
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "write failed",
            ));
        }
        Ok(())
    });
    let error = store.complete(&db, &Settings::default()).unwrap_err();
    on_stage(|_, _| Ok(()));
    assert_eq!(
        error,
        SettingsError::Io {
            kind: io::ErrorKind::ConnectionAborted,
            message: "write failed".to_owned(),
        }
    );
    assert!(pending_file(&path).is_dir());
    assert_eq!(fs::read_to_string(&path).unwrap(), HAND_EDIT);
    assert!(db.settings_defaults().unwrap().is_empty());
}

#[test]
fn ambiguous_prepared_recovery_keeps_every_pending_key_as_the_owners() {
    for (sort, font_size) in [
        (SortKey::Name, 13),
        (SortKey::Date, 18),
        (SortKey::Size, 20),
    ] {
        let dir = tmp();
        let path = settings_file(&dir);
        let db = ledger();
        let (mut store, _) = SettingsStore::load(&path);
        store.complete(&db, &Settings::default()).unwrap();
        let mut defaults = sorted_by(SortKey::Date);
        defaults.appearance.font_size = 18;
        crash_completion(&mut store, &db, &defaults, Stage::BeforeRename);
        let mut mine = sorted_by(sort);
        mine.appearance.font_size = font_size;
        let hand_edit = format!("# mine\n{}", complete_text("", &mine).unwrap().text);
        write(&path, &hand_edit);
        let (mut next, _) = SettingsStore::load(&path);
        let done = next.complete(&db, &mine).unwrap();
        assert!(
            matches!(done.ledger_error, Some(SettingsError::Ledger(ref message)) if message.contains("ambiguous"))
        );
        assert!(!pending_file(&path).exists());
        assert_eq!(fs::read_to_string(&path).unwrap(), hand_edit);
        defaults.files.sort = SortKey::Kind;
        defaults.appearance.font_size = 19;
        let (mut later, _) = SettingsStore::load(&path);
        let done = later.complete(&db, &defaults).unwrap();
        assert!(done.changes.is_empty());
        assert_eq!(later.current().files.sort, sort);
        assert_eq!(later.current().appearance.font_size, font_size);
        assert_eq!(fs::read_to_string(&path).unwrap(), hand_edit);
    }
}

#[test]
fn failed_pending_cleanup_replays_committed_rows_idempotently() {
    let dir = tmp();
    let path = settings_file(&dir);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&db, &Settings::default()).unwrap();
    let before = fs::read_to_string(&path).unwrap();
    on_stage(|path, stage| {
        if path.file_name().unwrap() == "ferrite.toml.defaults-pending" && stage == Stage::Replaced
        {
            let pending: toml::Value = toml::from_str(&fs::read_to_string(path).unwrap()).unwrap();
            if pending.get("stage").and_then(toml::Value::as_str) == Some("committed") {
                fs::rename(path, path.with_extension("held")).unwrap();
                fs::create_dir(path).unwrap();
            }
        }
        Ok(())
    });
    let done = store.complete(&db, &sorted_by(SortKey::Date)).unwrap();
    on_stage(|_, _| Ok(()));
    assert!(matches!(done.ledger_error, Some(SettingsError::Io { .. })));
    assert_eq!(recorded_sort(&db).as_deref(), Some("\"date\""));
    let pending = pending_file(&path);
    assert!(pending.is_dir());
    fs::remove_dir(&pending).unwrap();
    fs::rename(pending.with_extension("held"), &pending).unwrap();
    db.connection()
        .execute("UPDATE settings_defaults SET written_at = 7", [])
        .unwrap();
    let recorded = db.settings_defaults().unwrap();
    break_ledger_writes(&db);
    write(&path, &before);
    let (mut next, _) = SettingsStore::load(&path);
    next.complete(&db, &sorted_by(SortKey::Date)).unwrap();
    assert_eq!(db.settings_defaults().unwrap(), recorded);
    let changed: i64 = db
        .connection()
        .query_row(
            "SELECT count(*) FROM settings_defaults WHERE written_at != 7",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(changed, 0);
    assert!(!pending.exists());
    next.complete(&db, &sorted_by(SortKey::Size)).unwrap();
    assert_eq!(next.current().files.sort, SortKey::Name);
}

#[test]
fn ledger_recovery_keeps_an_owner_value_equal_to_the_previous_default() {
    let dir = tmp();
    let path = settings_file(&dir);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&db, &Settings::default()).unwrap();
    break_ledger_writes(&db);
    store.complete(&db, &sorted_by(SortKey::Date)).unwrap();
    let mine = fs::read_to_string(&path)
        .unwrap()
        .replace("sort = \"date\"", "sort = \"name\"");
    write(&path, &mine);
    mend_ledger_writes(&db);
    let (mut next, _) = SettingsStore::load(&path);
    let done = next.complete(&db, &sorted_by(SortKey::Date)).unwrap();
    assert_eq!(next.current().files.sort, SortKey::Name);
    assert!(done.changes.is_empty());
    assert_eq!(recorded_sort(&db).as_deref(), Some("\"date\""));
}

#[test]
fn ledger_recovery_before_another_default_change_keeps_the_owner_value() {
    let dir = tmp();
    let path = settings_file(&dir);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&db, &Settings::default()).unwrap();
    break_ledger_writes(&db);
    store.complete(&db, &sorted_by(SortKey::Date)).unwrap();
    let mine = fs::read_to_string(&path)
        .unwrap()
        .replace("sort = \"date\"", "sort = \"name\"");
    write(&path, &mine);
    mend_ledger_writes(&db);
    let (mut next, _) = SettingsStore::load(&path);
    next.complete(&db, &sorted_by(SortKey::Size)).unwrap();
    assert_eq!(next.current().files.sort, SortKey::Name);
    assert_eq!(recorded_sort(&db).as_deref(), Some("\"date\""));
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
    on_stage(|path, stage| {
        if stage == Stage::TempWritten {
            write(path, HAND_EDIT);
        }
        Ok(())
    });
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
    on_stage(|_, _| Ok(()));
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
    on_stage(|path, stage| {
        if path.file_name().unwrap() == "ferrite.toml" && stage == Stage::TempWritten {
            write(path, HAND_EDIT);
        }
        Ok(())
    });
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
    on_stage(|path, stage| {
        if stage == Stage::TempWritten {
            write(path, HAND_EDIT);
        }
        Ok(())
    });
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
    assert!(saved.was_present);
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
    fn a_noop_completion_succeeds_in_a_read_only_folder() {
        let dir = tmp();
        let path = settings_file(&dir);
        let before = complete_text("", &Settings::default()).unwrap().text;
        write(&path, &before);
        set_mode(&path, 0o644);
        let db = ledger();
        let (mut store, _) = SettingsStore::load(&path);
        set_mode(dir.path(), 0o555);
        let done = store.complete(&db, &Settings::default());
        set_mode(dir.path(), 0o755);
        assert_eq!(done.unwrap(), Completion::default());
        assert_eq!(db.settings_defaults().unwrap().len(), 25);
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
        assert_eq!(mode_of(&path), 0o644);
        assert!(!pending_file(&path).exists());
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
