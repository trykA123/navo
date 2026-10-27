use super::*;
use crate::table::NO_PARENT;

#[test]
fn totals_follow_insert_resize_move_and_subtree_removal() {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let old = table.push(root, "old", Flags::FOLDER).unwrap();
    let new = table.push(root, "new", Flags::FOLDER).unwrap();
    let branch = table.push(old, "branch", Flags::FOLDER).unwrap();
    let leaf = table.push(branch, "leaf", Flags::FOLDER).unwrap();
    let file = table.push(leaf, "file", Flags::NONE).unwrap();
    table.set_size(file, 100).unwrap();
    assert_eq!(
        table.totals(root),
        Some(Totals {
            bytes: 100,
            files: 1,
            folders: 4
        })
    );
    table.rename(branch, new, "moved").unwrap();
    assert_eq!(table.totals(old), Some(Totals::default()));
    assert_eq!(
        table.totals(new),
        Some(Totals {
            bytes: 100,
            files: 1,
            folders: 2
        })
    );
    assert_eq!(table.find_folder("c:/NEW/moved/leaf/"), Some(leaf));
    table.set_size(file, 25).unwrap();
    assert_eq!(table.totals(root).unwrap().bytes, 25);
    assert_eq!(table.totals(new).unwrap().bytes, 25);
    table.set_size(file, 300).unwrap();
    assert_eq!(table.totals(root).unwrap().bytes, 300);
    assert_eq!(table.remove(branch), 3);
    assert_eq!(table.totals(new), Some(Totals::default()));
    assert_eq!(
        table.totals(root),
        Some(Totals {
            bytes: 0,
            files: 0,
            folders: 2
        })
    );
    assert_eq!(table.size(file), None);
}

#[test]
fn late_parents_and_compaction_keep_subtree_totals() {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let file = table.push(2, "file", Flags::NONE).unwrap();
    table.set_size(file, 10).unwrap();
    let folder = table.push(root, "folder", Flags::FOLDER).unwrap();
    assert_eq!(
        table.totals(root),
        Some(Totals {
            bytes: 10,
            files: 1,
            folders: 1
        })
    );
    table.compact();
    assert_eq!(
        table.totals(folder),
        Some(Totals {
            bytes: 10,
            files: 1,
            folders: 0
        })
    );
    table.set_size(file, 20).unwrap();
    assert_eq!(table.totals(root).unwrap().bytes, 20);
}

#[test]
fn folder_lookup_uses_unicode_case_folding() {
    let mut table = NameTable::new();
    table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let folder = table.push(0, "Ä", Flags::FOLDER).unwrap();
    assert_eq!(table.find_folder("c:/ä"), Some(folder));
}

#[test]
fn files_do_not_allocate_folder_counts() {
    let mut table = NameTable::new();
    table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    for _ in 0..1000 {
        table.push(0, "file", Flags::NONE).unwrap();
    }
    assert_eq!(table.counts.len(), 1);
    let folder = table.push(0, "folder", Flags::FOLDER).unwrap();
    assert_eq!(table.counts.len(), 2);
    assert_eq!(table.totals(0).unwrap().files, 1000);
    table.remove(folder);
    table.compact();
    assert_eq!(table.counts.len(), 1);
}
