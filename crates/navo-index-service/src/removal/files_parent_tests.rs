//! Fake-filesystem tests for the change lock and the install parent after removal.

use super::*;

#[test]
fn removal_leaves_no_parent_after_the_lock_handle_closes() {
    let lock = parent().join(CHANGE_LOCK);
    let mut fake = Fake::with(&["navo.exe", "navo-index.exe"]);
    fake.files.insert(lock);
    let cleanup = clean_navo(&mut fake).unwrap();
    assert!(cleanup.is_complete(), "{cleanup:?}");
    assert!(fake.files.contains(&parent().join(CHANGE_LOCK)));
    assert!(fake.folders.contains(&parent()));
    remove_navo_parent(&mut fake);
    assert!(fake.files.is_empty(), "{:?}", fake.files);
    assert!(fake.folders.is_empty(), "{:?}", fake.folders);
}

#[test]
fn removal_deletes_an_empty_index_and_the_parent_after_the_lock_is_released() {
    let mut fake = Fake::with(&[]);
    fake.files.insert(parent().join(CHANGE_LOCK));
    remove_navo_parent(&mut fake);
    assert!(fake.files.is_empty(), "{:?}", fake.files);
    assert!(fake.folders.is_empty(), "{:?}", fake.folders);
}

#[test]
fn removal_keeps_an_unknown_file_and_its_parent_after_the_lock_is_released() {
    let unknown = parent().join("readme.txt");
    let mut fake = Fake::with(&["navo.exe"]);
    fake.files.insert(parent().join(CHANGE_LOCK));
    fake.files.insert(unknown.clone());
    let cleanup = clean_navo(&mut fake).unwrap();
    assert_eq!(cleanup.unknown, std::slice::from_ref(&unknown));
    remove_navo_parent(&mut fake);
    assert_eq!(fake.files, BTreeSet::from([unknown]));
    assert!(fake.folders.contains(&parent()));
}

#[test]
fn a_held_change_lock_is_left_alone_by_the_best_effort_cleanup() {
    let lock = parent().join(CHANGE_LOCK);
    let mut fake = Fake::with(&["navo.exe"]);
    fake.files.insert(lock.clone());
    fake.held_lock = true;
    let cleanup = clean_navo(&mut fake).unwrap();
    assert!(cleanup.is_complete(), "{cleanup:?}");
    fake.deletes = 0;
    let (parent, index) = (parent(), index());
    let folders = Folders {
        parent: &parent,
        index: &index,
        lock: Some(CHANGE_LOCK),
    };
    remove_parent(&mut fake, folders);
    assert!(fake.files.contains(&lock));
    assert!(fake.folders.contains(&parent));
    assert_eq!(
        fake.deletes, 0,
        "cleanup must never delete change.lock by name"
    );
}

#[test]
fn a_scheduled_index_schedules_its_parent_after_it() {
    let mut fake = Fake::with(&["navo-index.exe"]);
    fake.files.insert(parent().join(CHANGE_LOCK));
    fake.refuse("navo-index.exe", u32::MAX);
    let cleanup = clean_navo(&mut fake).unwrap();
    let renamed = index().join(format!("navo-index.exe.{PID}.old"));
    assert_eq!(fake.scheduled, [renamed, index(), parent()]);
    assert_eq!(cleanup.scheduled, fake.scheduled);
    assert_eq!(cleanup.outcome(), crate::Elevated::Deferred);
}
