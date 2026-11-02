use crate::Db;

use super::*;

const SORT: &str = "files.sort";

fn sorted_by(sort: SortKey) -> Settings {
    let mut settings = Settings::default();
    settings.files.sort = sort;
    settings
}

fn completed(dir: &TempDir, db: &Db) -> SettingsStore {
    let (mut store, _) = SettingsStore::load(&settings_file(dir));
    store.complete(db, &Settings::default()).unwrap();
    store
}

fn recorded(db: &Db, key: &str) -> Option<String> {
    db.settings_defaults().unwrap().get(key).cloned()
}

#[test]
fn a_completed_file_has_no_value_of_the_users() {
    let dir = tmp();
    let store = completed(&dir, &Db::open_in_memory().unwrap());
    assert!(!store.is_users(SORT, &Settings::default()));
    assert!(!store.is_users(MODE, &Settings::default()));
}

#[test]
fn a_value_set_in_the_sheet_is_the_users() {
    let dir = tmp();
    let mut store = completed(&dir, &Db::open_in_memory().unwrap());
    store.save(SORT, Some(text("size"))).unwrap();
    assert!(store.is_users(SORT, &Settings::default()));
    store.save(SORT, Some(text("name"))).unwrap();
    assert!(!store.is_users(SORT, &Settings::default()));
}

#[test]
fn a_value_the_ledger_follows_is_not_the_users_even_off_the_default() {
    let dir = tmp();
    let store = completed(&dir, &Db::open_in_memory().unwrap());
    assert!(!store.is_users(SORT, &sorted_by(SortKey::Date)));
}

#[test]
fn a_hand_edit_reloaded_from_disk_is_the_users() {
    let dir = tmp();
    let path = settings_file(&dir);
    let mut store = completed(&dir, &Db::open_in_memory().unwrap());
    let text = fs::read_to_string(&path).unwrap();
    write(&path, &text.replace("sort = \"name\"", "sort = \"kind\""));
    store.reload();
    assert!(store.is_users(SORT, &Settings::default()));
}

#[test]
fn a_key_without_a_default_is_the_users_once_set() {
    let dir = tmp();
    let mut store = completed(&dir, &Db::open_in_memory().unwrap());
    assert!(!store.is_users("editor.path", &Settings::default()));
    store.save("editor.path", Some(text("code"))).unwrap();
    assert!(store.is_users("editor.path", &Settings::default()));
}

#[test]
fn reset_writes_the_current_default_and_the_ledger_follows_it_again() {
    let dir = tmp();
    let db = Db::open_in_memory().unwrap();
    let mut store = completed(&dir, &db);
    store.save(SORT, Some(text("size"))).unwrap();
    let newer = sorted_by(SortKey::Date);
    let saved = store.reset(SORT, Some(&db), &newer).unwrap();
    assert_eq!(saved.ledger_error, None);
    assert_eq!(store.current().files.sort, SortKey::Date);
    assert!(
        fs::read_to_string(settings_file(&dir))
            .unwrap()
            .contains("sort = \"date\"\n")
    );
    assert_eq!(recorded(&db, SORT).as_deref(), Some("\"date\""));
    assert!(!store.is_users(SORT, &newer));
}

#[test]
fn reset_of_a_key_without_a_default_removes_it() {
    let dir = tmp();
    let db = Db::open_in_memory().unwrap();
    let mut store = completed(&dir, &db);
    store.save("editor.path", Some(text("code"))).unwrap();
    store
        .reset("editor.path", Some(&db), &Settings::default())
        .unwrap();
    assert!(
        !fs::read_to_string(settings_file(&dir))
            .unwrap()
            .contains("code")
    );
    assert!(!store.is_users("editor.path", &Settings::default()));
    assert_eq!(recorded(&db, "editor.path"), None);
}

#[test]
fn reset_without_a_ledger_still_writes_the_default() {
    let dir = tmp();
    let mut store = completed(&dir, &Db::open_in_memory().unwrap());
    store.save(SORT, Some(text("size"))).unwrap();
    store.reset(SORT, None, &Settings::default()).unwrap();
    assert_eq!(store.current().files.sort, SortKey::Name);
    assert!(!store.is_users(SORT, &Settings::default()));
}

#[test]
fn a_missing_key_lands_on_its_section_header_or_the_last_line() {
    let text = "# mine\n[files]\nsort = \"size\"\n\n[appearance]\ndensity = \"compact\"\n";
    assert_eq!(section_line(text, MODE), 5);
    assert_eq!(section_line(text, "terminal.provider"), 6);
    assert_eq!(section_line("", MODE), 1);
}
