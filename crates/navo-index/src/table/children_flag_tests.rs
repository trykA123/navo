use super::super::compact::Remap;
use super::super::{Flags, NameTable, TableChange, Totals};
use crate::file::{FileError, Header, load_with_ids, save_with_ids};
use crate::usn::FrnMap;

const ROOT: u128 = 5;

fn add(file_id: u128, parent_id: u128, name: &str, flags: Flags) -> TableChange {
    TableChange::Add {
        file_id,
        parent_id,
        name: name.into(),
        flags,
    }
}

fn root() -> (NameTable, FrnMap) {
    let mut table = NameTable::new();
    let mut ids = FrnMap::new();
    table
        .apply(add(ROOT, ROOT, "C:", Flags::FOLDER), &mut ids)
        .unwrap();
    (table, ids)
}

#[expect(
    clippy::too_many_arguments,
    reason = "test helper mirrors the fields of one Add change"
)]
fn entry(
    table: &mut NameTable,
    ids: &mut FrnMap,
    file_id: u128,
    parent_id: u128,
    name: &str,
    flags: Flags,
) -> u32 {
    table
        .apply(add(file_id, parent_id, name, flags), ids)
        .unwrap();
    ids[&file_id]
}

fn assert_children_after_compaction(table: &NameTable, parent: u32, expected: &[&str]) {
    let mut compacted = table.clone();
    let map: Remap = compacted.compact();
    let parent = map.get(parent).unwrap();
    assert_eq!(compacted.child_names(parent), expected);
}

fn assert_children_after_load(table: &NameTable, ids: &FrnMap, parent: u32, expected: &[&str]) {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("children.index");
    let header = Header {
        volume_serial: 1,
        journal_id: 1,
        next_usn: 0,
        root: "C:".to_owned(),
        built_at: 0,
    };
    save_with_ids(&path, &header, table, ids).unwrap();
    let (_, loaded, _) = load_with_ids(&path).unwrap();
    assert_eq!(loaded.child_names(parent), expected);
}

#[test]
fn attributes_promotes_a_file_and_a_later_child_is_listed_and_removed() {
    let (mut table, mut ids) = root();
    let file = entry(&mut table, &mut ids, 10, ROOT, "file.txt", Flags::NONE);
    assert!(table.child_names(file).is_empty());
    table
        .apply(
            TableChange::Attributes {
                file_id: 10,
                flags: Flags::FOLDER,
            },
            &mut ids,
        )
        .unwrap();
    entry(&mut table, &mut ids, 11, 10, "child.txt", Flags::NONE);
    assert_eq!(table.child_names(file), ["child.txt"]);
    assert_eq!(
        table.totals(file),
        Some(Totals {
            bytes: 0,
            files: 1,
            folders: 0,
        })
    );
    assert_eq!(
        table.totals(0),
        Some(Totals {
            bytes: 0,
            files: 1,
            folders: 1,
        })
    );
    assert_children_after_compaction(&table, file, &["child.txt"]);
    assert_eq!(table.remove(file), 2);
    assert_eq!(table.len(), 1);
    assert_eq!(table.totals(0), Some(Totals::default()));
}

#[test]
fn add_with_folder_on_a_known_reference_turns_it_into_a_folder_that_holds_children() {
    let (mut table, mut ids) = root();
    let file = entry(&mut table, &mut ids, 10, ROOT, "file.txt", Flags::NONE);
    table
        .apply(add(10, ROOT, "renamed.txt", Flags::FOLDER), &mut ids)
        .unwrap();
    assert_eq!(table.name(file), Some("renamed.txt"));
    assert_eq!(table.flags(file), Some(Flags::FOLDER));
    entry(&mut table, &mut ids, 11, 10, "child.txt", Flags::NONE);
    assert_eq!(table.child_names(file), ["child.txt"]);
    assert_eq!(
        table.totals(file),
        Some(Totals {
            bytes: 0,
            files: 1,
            folders: 0,
        })
    );
    assert_eq!(
        table.totals(0),
        Some(Totals {
            bytes: 0,
            files: 1,
            folders: 1,
        })
    );
    assert_children_after_compaction(&table, file, &["child.txt"]);
    assert_eq!(table.remove(file), 2);
}

#[test]
fn attributes_demoted_folder_keeps_its_children_and_removal_takes_the_subtree() {
    let (mut table, mut ids) = root();
    let folder = entry(&mut table, &mut ids, 10, ROOT, "folder", Flags::FOLDER);
    let file = entry(&mut table, &mut ids, 11, 10, "file.txt", Flags::NONE);
    entry(&mut table, &mut ids, 12, 10, "sub", Flags::FOLDER);
    entry(&mut table, &mut ids, 13, 12, "deep.txt", Flags::NONE);
    table.set_size(file, 100).unwrap();
    assert_eq!(table.child_names(folder), ["sub", "file.txt"]);
    assert_eq!(
        table.totals(0),
        Some(Totals {
            bytes: 100,
            files: 2,
            folders: 2,
        })
    );
    table
        .apply(
            TableChange::Attributes {
                file_id: 10,
                flags: Flags::NONE,
            },
            &mut ids,
        )
        .unwrap();
    assert_eq!(table.totals(folder), None);
    assert_eq!(table.child_names(folder), ["sub", "file.txt"]);
    assert_eq!(table.totals(0).unwrap().files, 2);
    assert_children_after_compaction(&table, folder, &["sub", "file.txt"]);
    assert_eq!(table.remove(folder), 4);
    let after = table.totals(0).unwrap();
    assert_eq!(
        after,
        Totals {
            bytes: 0,
            files: 1,
            folders: 2,
        }
    );
}

#[test]
fn rename_of_a_child_into_an_id_pushed_as_a_file_lists_it_there() {
    let (mut table, mut ids) = root();
    let folder = entry(&mut table, &mut ids, 10, ROOT, "folder", Flags::FOLDER);
    let file = entry(&mut table, &mut ids, 11, ROOT, "file.txt", Flags::NONE);
    let child = entry(&mut table, &mut ids, 12, 10, "child.txt", Flags::NONE);
    assert_eq!(table.child_names(folder), ["child.txt"]);
    table
        .apply(
            TableChange::Rename {
                file_id: 12,
                parent_id: 11,
                name: "moved.txt".into(),
                flags: Flags::NONE,
            },
            &mut ids,
        )
        .unwrap();
    assert_eq!(table.parent(child), Some(file));
    assert_eq!(table.child_names(folder), Vec::<&str>::new());
    assert_eq!(table.child_names(file), ["moved.txt"]);
    assert_eq!(
        table.totals(0),
        Some(Totals {
            bytes: 0,
            files: 2,
            folders: 1,
        })
    );
    assert_children_after_compaction(&table, file, &["moved.txt"]);
    assert_eq!(table.remove(file), 2);
    assert_eq!(
        table.totals(0),
        Some(Totals {
            bytes: 0,
            files: 1,
            folders: 1,
        })
    );
}

#[test]
fn reparenting_between_two_folders_and_back_keeps_children_and_totals() {
    let (mut table, mut ids) = root();
    let left = entry(&mut table, &mut ids, 10, ROOT, "left", Flags::FOLDER);
    let right = entry(&mut table, &mut ids, 11, ROOT, "right", Flags::FOLDER);
    let child = entry(&mut table, &mut ids, 12, 10, "child.txt", Flags::NONE);
    assert_eq!(table.totals(left).unwrap().files, 1);
    assert_eq!(table.totals(right).unwrap().files, 0);
    table
        .apply(
            TableChange::Rename {
                file_id: 12,
                parent_id: 11,
                name: "child.txt".into(),
                flags: Flags::NONE,
            },
            &mut ids,
        )
        .unwrap();
    assert_eq!(table.parent(child), Some(right));
    assert!(table.child_names(left).is_empty());
    assert_eq!(table.child_names(right), ["child.txt"]);
    assert_eq!(table.totals(left).unwrap().files, 0);
    assert_eq!(table.totals(right).unwrap().files, 1);
    assert_children_after_compaction(&table, right, &["child.txt"]);
    table
        .apply(
            TableChange::Rename {
                file_id: 12,
                parent_id: 10,
                name: "child.txt".into(),
                flags: Flags::NONE,
            },
            &mut ids,
        )
        .unwrap();
    assert_eq!(table.parent(child), Some(left));
    assert_eq!(table.child_names(left), ["child.txt"]);
    assert!(table.child_names(right).is_empty());
    assert_eq!(table.totals(left).unwrap().files, 1);
    assert_eq!(table.totals(right).unwrap().files, 0);
    assert_children_after_compaction(&table, left, &["child.txt"]);
    assert_eq!(table.remove(right), 1);
}

#[test]
fn a_file_promoted_to_a_folder_loads_with_its_children() {
    let (mut table, mut ids) = root();
    let file = entry(&mut table, &mut ids, 10, ROOT, "file.txt", Flags::NONE);
    table
        .apply(
            TableChange::Attributes {
                file_id: 10,
                flags: Flags::FOLDER,
            },
            &mut ids,
        )
        .unwrap();
    entry(&mut table, &mut ids, 11, 10, "child.txt", Flags::NONE);
    assert_children_after_load(&table, &ids, file, &["child.txt"]);
}

#[test]
fn an_upserted_file_promoted_to_a_folder_loads_with_its_children() {
    let (mut table, mut ids) = root();
    let file = entry(&mut table, &mut ids, 10, ROOT, "file.txt", Flags::NONE);
    table
        .apply(add(10, ROOT, "renamed.txt", Flags::FOLDER), &mut ids)
        .unwrap();
    entry(&mut table, &mut ids, 11, 10, "child.txt", Flags::NONE);
    assert_children_after_load(&table, &ids, file, &["child.txt"]);
}

#[test]
fn a_rename_that_gains_the_folder_flag_restores_stored_totals() {
    let (mut table, mut ids) = root();
    let folder = entry(&mut table, &mut ids, 10, ROOT, "folder", Flags::NONE);
    let child = entry(&mut table, &mut ids, 11, 10, "child.txt", Flags::NONE);
    table.set_size(child, 100).unwrap();
    table
        .apply(
            TableChange::Rename {
                file_id: 10,
                parent_id: ROOT,
                name: "folder".into(),
                flags: Flags::FOLDER,
            },
            &mut ids,
        )
        .unwrap();
    assert_eq!(
        table.totals(folder),
        Some(Totals {
            bytes: 100,
            files: 1,
            folders: 0,
        })
    );
    assert_eq!(
        table.totals(0),
        Some(Totals {
            bytes: 100,
            files: 1,
            folders: 1,
        })
    );
    let (sizes, counts) = table.stored_totals();
    let (sizes, counts) = (sizes.to_vec(), counts.collect::<Vec<_>>());
    let mut restored = table.clone();
    assert!(restored.restore_totals(sizes.clone(), counts.clone()));
    assert_eq!(restored.stored_totals().0, sizes);
    assert_eq!(restored.stored_totals().1.collect::<Vec<_>>(), counts);
}

#[test]
fn an_empty_folder_demoted_to_a_file_loads_again() {
    let (mut table, mut ids) = root();
    let folder = entry(&mut table, &mut ids, 10, ROOT, "folder", Flags::FOLDER);
    let file = entry(&mut table, &mut ids, 11, ROOT, "file.txt", Flags::NONE);
    table.set_size(file, 50).unwrap();
    table
        .apply(
            TableChange::Attributes {
                file_id: 10,
                flags: Flags::NONE,
            },
            &mut ids,
        )
        .unwrap();
    assert_eq!(table.totals(folder), None);
    assert_eq!(
        table.totals(0),
        Some(Totals {
            bytes: 50,
            files: 2,
            folders: 0,
        })
    );
    assert_children_after_load(&table, &ids, folder, &[]);
}

#[test]
fn a_folder_demoted_to_a_file_with_children_is_refused_on_load() {
    let (mut table, mut ids) = root();
    entry(&mut table, &mut ids, 10, ROOT, "folder", Flags::FOLDER);
    entry(&mut table, &mut ids, 11, 10, "child.txt", Flags::NONE);
    table
        .apply(
            TableChange::Attributes {
                file_id: 10,
                flags: Flags::NONE,
            },
            &mut ids,
        )
        .unwrap();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("demoted.index");
    let header = Header {
        volume_serial: 1,
        journal_id: 1,
        next_usn: 0,
        root: "C:".to_owned(),
        built_at: 0,
    };
    save_with_ids(&path, &header, &table, &ids).unwrap();
    assert!(matches!(
        load_with_ids(&path),
        Err(FileError::InvalidData(_))
    ));
}
