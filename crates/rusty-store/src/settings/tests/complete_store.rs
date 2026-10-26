use crate::Db;

use super::*;

fn ledger() -> Db {
    Db::open_in_memory().unwrap()
}

fn sorted_by_date() -> Settings {
    let mut settings = Settings::default();
    settings.files.sort = SortKey::Date;
    settings
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

#[test]
fn a_missing_file_is_written_whole_and_every_default_recorded() {
    let dir = tmp();
    let path = settings_file(&dir);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    let done = store.complete(&db, &Settings::default()).unwrap();
    assert!(done.written);
    assert_eq!(done.changes, []);
    assert_eq!(
        read(&path),
        to_toml(&Settings::default())
            .replace("\n[editor]\n", "")
            .replace("\n\n[experimental]\n", "\n")
    );
    let recorded = db.settings_defaults().unwrap();
    assert_eq!(recorded.len(), 59);
    assert_eq!(
        recorded.get("files.sort").map(String::as_str),
        Some("\"name\"")
    );
}

#[test]
fn an_empty_file_is_completed_and_backed_up_first() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "");
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&ledger(), &Settings::default()).unwrap();
    assert!(read(&path).contains("[terminal]\nprovider = \"windows-terminal\"\n"));
    assert_eq!(read(&backup_path(&path)), "");
}

#[test]
fn a_complete_file_is_not_written_again() {
    let dir = tmp();
    let path = settings_file(&dir);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&db, &Settings::default()).unwrap();
    let before = fs::read(&path).unwrap();
    let (mut next, _) = SettingsStore::load(&path);
    let done = next.complete(&db, &Settings::default()).unwrap();
    assert!(!done.written);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!backup_path(&path).exists());
}

#[test]
fn an_unparsable_file_is_left_alone_and_nothing_recorded() {
    for broken in [
        "[appearance\nmode = \"dark\"\n",
        "[appearance]\nfont-size = \"big\"\n",
    ] {
        let dir = tmp();
        let path = settings_file(&dir);
        write(&path, broken);
        let db = ledger();
        let (mut store, _) = SettingsStore::load(&path);
        assert_eq!(
            store.complete(&db, &Settings::default()),
            Err(SettingsError::FileInvalid)
        );
        assert_eq!(fs::read(&path).unwrap(), broken.as_bytes());
        assert!(!backup_path(&path).exists());
        assert!(db.settings_defaults().unwrap().is_empty());
        assert_eq!(store.current(), &Settings::default());
    }
}

#[test]
fn a_following_key_follows_a_new_default_and_the_change_is_listed() {
    let dir = tmp();
    let path = settings_file(&dir);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&db, &Settings::default()).unwrap();
    let (mut next, _) = SettingsStore::load(&path);
    let done = next.complete(&db, &sorted_by_date()).unwrap();
    assert!(done.written);
    assert_eq!(
        done.changes,
        [DefaultChange {
            key: "files.sort".to_owned(),
            old: "\"name\"".to_owned(),
            new: "\"date\"".to_owned(),
        }]
    );
    assert!(read(&path).contains("sort = \"date\"\n"));
    assert_eq!(next.current().files.sort, SortKey::Date);
    assert_eq!(
        db.settings_defaults()
            .unwrap()
            .get("files.sort")
            .map(String::as_str),
        Some("\"date\"")
    );
}

#[test]
fn a_key_changed_in_the_sheet_keeps_its_value_after_an_upgrade() {
    let dir = tmp();
    let path = settings_file(&dir);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&db, &Settings::default()).unwrap();
    store.save("files.sort", Some(text("size"))).unwrap();
    let before = fs::read(&path).unwrap();
    let (mut next, _) = SettingsStore::load(&path);
    let done = next.complete(&db, &sorted_by_date()).unwrap();
    assert_eq!(done, Completion::default());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(next.current().files.sort, SortKey::Size);
}

#[test]
fn a_file_from_before_the_ledger_keeps_user_values_and_records_defaults() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(
        &path,
        "# mine\n[files]\nsort = \"size\"\nfolders-first = true\n",
    );
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&db, &Settings::default()).unwrap();
    let recorded = db.settings_defaults().unwrap();
    assert_eq!(recorded.get("files.sort"), None);
    assert_eq!(
        recorded.get("files.folders-first").map(String::as_str),
        Some("true")
    );
    assert!(read(&path).starts_with("# mine\n[files]\nsort = \"size\"\nfolders-first = true\n"));
}

#[test]
fn the_backup_keeps_the_file_from_before_the_session_after_completion_and_a_save() {
    let dir = tmp();
    let path = settings_file(&dir);
    let old = "# before\n[appearance]\nmode = \"light\"\n";
    write(&path, old);
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&ledger(), &Settings::default()).unwrap();
    store.save(MODE, Some(text("dark"))).unwrap();
    assert_eq!(read(&backup_path(&path)), old);
}

#[test]
fn completion_keeps_crlf_bom_and_comments_on_disk() {
    let dir = tmp();
    let path = settings_file(&dir);
    let before = "\u{feff}# mine\r\n[appearance]\r\nmode = \"dark\" # night\r\n";
    write(&path, before);
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&ledger(), &Settings::default()).unwrap();
    let after = read(&path);
    assert!(after.starts_with(before), "{after:?}");
    assert_eq!(after.matches("\r\n").count(), after.matches('\n').count());
    assert_eq!(store.current().appearance.mode, ThemeMode::Dark);
}

#[test]
fn a_removed_key_is_noted_once_on_disk() {
    let dir = tmp();
    let path = settings_file(&dir);
    write(&path, "[files]\nshow-hidden = true\n");
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&db, &Settings::default()).unwrap();
    let first = read(&path);
    let (mut next, _) = SettingsStore::load(&path);
    let done = next.complete(&db, &Settings::default()).unwrap();
    assert!(!done.written);
    assert_eq!(read(&path), first);
    assert_eq!(first.matches(UNUSED_NOTE).count(), 1, "{first}");
}

#[test]
fn a_key_deleted_by_hand_comes_back_with_the_current_default() {
    let dir = tmp();
    let path = settings_file(&dir);
    let db = ledger();
    let (mut store, _) = SettingsStore::load(&path);
    store.complete(&db, &Settings::default()).unwrap();
    write(&path, &read(&path).replace("sort = \"name\"\n", ""));
    let (mut next, _) = SettingsStore::load(&path);
    let done = next.complete(&db, &sorted_by_date()).unwrap();
    assert!(read(&path).contains("sort = \"date\"\n"));
    assert_eq!(done.changes, []);
    assert_eq!(
        db.settings_defaults()
            .unwrap()
            .get("files.sort")
            .map(String::as_str),
        Some("\"date\"")
    );
}
