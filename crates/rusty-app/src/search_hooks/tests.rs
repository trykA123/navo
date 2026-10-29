use std::sync::{Arc, Mutex};

use rusty_store::Db;

use super::hooks;

fn shared() -> crate::files::SharedDb {
    Arc::new(Mutex::new(
        rusty_store::JobJournal::new(Db::open_in_memory().unwrap()).unwrap(),
    ))
}

fn remembering() -> super::Remember {
    super::Remember::default()
}

#[test]
fn recorded_commands_come_back_with_the_most_used_first() {
    let hooks = hooks(Some(shared()), remembering());
    (hooks.record_command)("files.refresh");
    (hooks.record_command)("pane.switch");
    (hooks.record_command)("pane.switch");
    let used = (hooks.command_use)();
    let names: Vec<&str> = used.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["pane.switch", "files.refresh"]);
    assert!(used[0].1 > used[1].1);
}

#[test]
fn clearing_forgets_command_use() {
    let hooks = hooks(Some(shared()), remembering());
    (hooks.record_command)("files.refresh");
    (hooks.clear)();
    assert!((hooks.command_use)().is_empty());
}

#[test]
fn a_failed_write_is_swallowed_and_without_a_database_nothing_happens() {
    let db = shared();
    crate::files::lock(&db)
        .connection()
        .execute_batch("DROP TABLE command_use")
        .unwrap();
    let broken = hooks(Some(db), remembering());
    (broken.record_command)("files.refresh");
    assert!((broken.command_use)().is_empty());
    let none = hooks(None, remembering());
    (none.record_command)("files.refresh");
    assert!((none.command_use)().is_empty());
}

#[test]
fn visits_come_back_most_frecent_first() {
    let hooks = hooks(Some(shared()), remembering());
    (hooks.record_visit)("/home/docs", true);
    (hooks.record_visit)("/home/a.txt", false);
    (hooks.record_visit)("/home/docs", true);
    let found = (hooks.frecent)(10);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].location, "/home/docs");
    assert!(found[0].is_folder && !found[1].is_folder);
    (hooks.clear)();
    assert!((hooks.frecent)(10).is_empty());
}

#[test]
fn with_remember_off_nothing_is_recorded_and_turning_it_on_resumes() {
    let remember = remembering();
    let hooks = hooks(Some(shared()), remember.clone());
    remember.set(false);
    (hooks.record_visit)("/home/docs", true);
    (hooks.record_command)("files.refresh");
    assert!((hooks.frecent)(10).is_empty());
    assert!((hooks.command_use)().is_empty());
    remember.set(true);
    (hooks.record_visit)("/home/docs", true);
    assert_eq!((hooks.frecent)(10).len(), 1);
}

#[test]
fn with_remember_off_stored_history_is_neither_used_nor_shown() {
    let remember = remembering();
    let hooks = hooks(Some(shared()), remember.clone());
    (hooks.record_visit)("/home/docs", true);
    (hooks.record_command)("files.refresh");
    remember.set(false);
    assert!((hooks.frecent)(10).is_empty());
    assert!((hooks.command_use)().is_empty());
    assert!((hooks.recent_commands)(10).is_empty());
    remember.set(true);
    assert_eq!((hooks.frecent)(10).len(), 1);
    assert_eq!((hooks.command_use)().len(), 1);
    assert_eq!((hooks.recent_commands)(10).len(), 1);
}
