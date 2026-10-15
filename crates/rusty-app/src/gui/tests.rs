use rusty_core::Location;
use rusty_fs::FakeFs;
use rusty_store::StoreDirs;

use super::operations;

#[test]
fn without_store_folders_only_home_is_protected() {
    let home = Location::new("/home/admin").unwrap();
    let ops = operations(None, &home, &FakeFs::new());
    assert_eq!(ops.protected, [home]);
}

#[test]
fn store_folders_are_protected_and_get_a_journal() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = StoreDirs {
        config: dir.path().join("config"),
        data: dir.path().join("data"),
    };
    let home = Location::new("/home/admin").unwrap();
    let ops = operations(Some(&dirs), &home, &FakeFs::new());
    assert_eq!(ops.protected.len(), 3);
    assert!(dirs.data.join("ferrite.db").exists());
}
