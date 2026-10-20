use std::sync::{Arc, Mutex};

use rusty_store::Db;

use super::hooks;

fn shared() -> Arc<Mutex<Db>> {
    Arc::new(Mutex::new(Db::open_in_memory().unwrap()))
}

#[test]
fn recorded_commands_come_back_with_the_most_used_first() {
    let hooks = hooks(Some(shared()));
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
    let hooks = hooks(Some(shared()));
    (hooks.record_command)("files.refresh");
    (hooks.clear)();
    assert!((hooks.command_use)().is_empty());
}

#[test]
fn a_failed_write_is_swallowed_and_without_a_database_nothing_happens() {
    let db = shared();
    db.lock()
        .unwrap()
        .connection()
        .execute_batch("DROP TABLE command_use")
        .unwrap();
    let broken = hooks(Some(db));
    (broken.record_command)("files.refresh");
    assert!((broken.command_use)().is_empty());
    let none = hooks(None);
    (none.record_command)("files.refresh");
    assert!((none.command_use)().is_empty());
}
