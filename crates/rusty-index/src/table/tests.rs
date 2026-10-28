use super::{Flags, NO_PARENT, NameTable, TableChange, TableError};
use crate::fuzzy::Fuzzy;
use crate::usn::FrnMap;

fn tree() -> (NameTable, [u32; 5]) {
    let mut t = NameTable::new();
    let root = t.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let users = t.push(root, "Users", Flags::FOLDER).unwrap();
    let ann = t.push(users, "ann", Flags::FOLDER).unwrap();
    let notes = t.push(ann, "notes.txt", Flags::NONE).unwrap();
    let docs = t.push(ann, "docs", Flags::FOLDER).unwrap();
    (t, [root, users, ann, notes, docs])
}

fn win(t: &NameTable, id: u32) -> Option<String> {
    t.path_with(id, '\\')
}

fn add(file_id: u128, parent_id: u128, name: &str, flags: Flags) -> TableChange {
    TableChange::Add {
        file_id,
        parent_id,
        name: name.into(),
        flags,
    }
}

fn journal_tree() -> (NameTable, FrnMap) {
    let mut table = NameTable::new();
    let mut ids = FrnMap::new();
    for change in [
        add(7, 7, "C:", Flags::FOLDER),
        add(8, 7, "grove", Flags::FOLDER),
        add(42, 7, "ash", Flags::NONE),
    ] {
        table.apply(change, &mut ids).unwrap();
    }
    (table, ids)
}

#[test]
fn changes_map_full_file_references_to_dense_entry_ids() {
    let (mut table, mut ids) = journal_tree();
    let wide = (1u128 << 100) | 42;
    table
        .apply(add(wide, 8, "elm", Flags::HIDDEN), &mut ids)
        .unwrap();
    assert_eq!(ids[&7], 0);
    assert_eq!(ids[&42], 2);
    assert_ne!(ids[&42], ids[&wide]);
    assert_eq!(win(&table, ids[&wide]).as_deref(), Some("C:\\grove\\elm"));
    assert_eq!(table.flags(ids[&wide]), Some(Flags::HIDDEN));
}

#[test]
fn applying_duplicate_adds_keeps_the_existing_entry_id() {
    let (mut table, mut ids) = journal_tree();
    let id = ids[&42];
    table
        .apply(add(42, 8, "elm", Flags::SYSTEM), &mut ids)
        .unwrap();
    assert_eq!(ids[&42], id);
    assert_eq!(table.len(), 3);
    assert_eq!(win(&table, id).as_deref(), Some("C:\\grove\\elm"));
    assert_eq!(table.flags(id), Some(Flags::SYSTEM));
}

#[test]
fn applying_rename_moves_an_entry_and_updates_attributes() {
    let (mut table, mut ids) = journal_tree();
    let id = ids[&42];
    table
        .apply(
            TableChange::Rename {
                file_id: 42,
                parent_id: 8,
                name: "elm".into(),
                flags: Flags::HIDDEN,
            },
            &mut ids,
        )
        .unwrap();
    assert_eq!(ids[&42], id);
    assert_eq!(win(&table, id).as_deref(), Some("C:\\grove\\elm"));
    assert_eq!(table.flags(id), Some(Flags::HIDDEN));
}

#[test]
fn applying_attributes_clears_previous_flags_without_renaming() {
    let (mut table, mut ids) = journal_tree();
    for flags in [Flags::HIDDEN | Flags::SYSTEM, Flags::NONE] {
        table
            .apply(TableChange::Attributes { file_id: 42, flags }, &mut ids)
            .unwrap();
        assert_eq!(table.flags(ids[&42]), Some(flags));
        assert_eq!(table.name(ids[&42]), Some("ash"));
    }
}

#[test]
fn applying_remove_drops_descendants_and_their_file_reference_mappings() {
    let (mut table, mut ids) = journal_tree();
    table
        .apply(add(43, 8, "elm", Flags::NONE), &mut ids)
        .unwrap();
    table
        .apply(TableChange::Remove { file_id: 8 }, &mut ids)
        .unwrap();
    assert_eq!(table.len(), 2);
    assert!(!ids.contains_key(&8));
    assert!(!ids.contains_key(&43));
    table
        .apply(TableChange::Remove { file_id: 8 }, &mut ids)
        .unwrap();
    assert_eq!(table.len(), 2);
}

#[test]
fn applying_journal_reset_clears_the_table_and_file_references() {
    let (mut table, mut ids) = journal_tree();
    table.apply(TableChange::JournalReset, &mut ids).unwrap();
    assert!(table.is_empty());
    assert_eq!(table.slots(), 0);
    assert!(ids.is_empty());
}

#[test]
fn missing_parent_is_an_error_and_does_not_partly_apply_an_add() {
    let (mut table, mut ids) = journal_tree();
    assert_eq!(
        table.apply(add(43, 99, "elm", Flags::NONE), &mut ids),
        Err(TableError::UnknownFileReference(99))
    );
    assert_eq!(table.len(), 3);
    assert!(!ids.contains_key(&43));
}

#[test]
fn rename_cycle_and_unknown_file_references_leave_the_table_unchanged() {
    let (mut table, mut ids) = journal_tree();
    let root = ids[&7];
    assert_eq!(
        table.apply(
            TableChange::Rename {
                file_id: 7,
                parent_id: 8,
                name: "loop".into(),
                flags: Flags::NONE,
            },
            &mut ids
        ),
        Err(TableError::Cycle(root))
    );
    assert_eq!(table.name(root), Some("C:"));
    assert_eq!(table.flags(root), Some(Flags::FOLDER));
    assert_eq!(
        table.apply(
            TableChange::Attributes {
                file_id: 99,
                flags: Flags::HIDDEN,
            },
            &mut ids
        ),
        Err(TableError::UnknownFileReference(99))
    );
}

#[test]
fn push_assigns_dense_ids_and_reads_back() {
    let (t, [root, _, _, notes, _]) = tree();
    assert_eq!((root, notes), (0, 3));
    assert_eq!(t.len(), 5);
    assert_eq!(t.name(notes), Some("notes.txt"));
    assert_eq!(t.parent(notes), Some(2));
    assert_eq!(t.parent(root), None);
}

#[test]
fn path_joins_names_from_the_root() {
    let (t, [_, _, ann, notes, _]) = tree();
    assert_eq!(win(&t, notes).as_deref(), Some("C:\\Users\\ann\\notes.txt"));
    assert_eq!(t.path_with(ann, '/').as_deref(), Some("C:/Users/ann"));
}

#[test]
fn empty_root_name_gives_a_leading_separator() {
    let mut t = NameTable::new();
    let root = t.push(NO_PARENT, "", Flags::FOLDER).unwrap();
    let home = t.push(root, "home", Flags::FOLDER).unwrap();
    assert_eq!(t.path_with(home, '/').as_deref(), Some("/home"));
}

#[test]
fn children_pushed_before_their_parents_resolve_once_parents_arrive() {
    let mut t = NameTable::new();
    let notes = t.push(1, "notes.txt", Flags::NONE).unwrap();
    let ann = t.push(2, "ann", Flags::FOLDER).unwrap();
    assert_eq!(win(&t, notes).as_deref(), Some("ann\\notes.txt"));
    let users = t.push(NO_PARENT, "Users", Flags::FOLDER).unwrap();
    assert_eq!((notes, ann, users), (0, 1, 2));
    assert_eq!(win(&t, notes).as_deref(), Some("Users\\ann\\notes.txt"));
}

#[test]
fn a_missing_parent_gives_a_partial_path_and_a_bad_id_gives_none() {
    let mut t = NameTable::new();
    let orphan = t.push(77, "lost.txt", Flags::NONE).unwrap();
    assert_eq!(win(&t, orphan).as_deref(), Some("lost.txt"));
    assert_eq!(win(&t, 99), None);
    assert_eq!(t.name(99), None);
}

#[test]
fn rename_in_place_keeps_children_paths_right() {
    let (mut t, [_, users, ann, notes, docs]) = tree();
    t.rename(ann, users, "bob").unwrap();
    assert_eq!(win(&t, notes).as_deref(), Some("C:\\Users\\bob\\notes.txt"));
    assert_eq!(win(&t, docs).as_deref(), Some("C:\\Users\\bob\\docs"));
}

#[test]
fn move_keeps_children_paths_right() {
    let (mut t, [root, _, ann, notes, _]) = tree();
    t.rename(ann, root, "ann").unwrap();
    assert_eq!(win(&t, notes).as_deref(), Some("C:\\ann\\notes.txt"));
    assert_eq!(t.parent(ann), Some(root));
}

#[test]
fn moving_a_folder_into_its_own_subtree_is_refused() {
    let (mut t, [_, users, ann, _, docs]) = tree();
    assert_eq!(
        t.rename(users, docs, "Users"),
        Err(TableError::Cycle(users))
    );
    assert_eq!(t.rename(ann, ann, "ann"), Err(TableError::Cycle(ann)));
    assert_eq!(t.push(5, "self", Flags::NONE), Err(TableError::Cycle(5)));
}

#[test]
fn rename_of_a_missing_entry_fails() {
    let (mut t, _) = tree();
    assert_eq!(t.rename(42, NO_PARENT, "x"), Err(TableError::UnknownId(42)));
}

#[test]
fn renaming_to_the_same_name_does_not_grow_the_table() {
    let (mut t, [_, users, ann, ..]) = tree();
    let before = t.names.len();
    t.rename(ann, users, "ann").unwrap();
    assert_eq!(t.names.len(), before);
}

#[test]
fn remove_a_file_leaves_siblings() {
    let (mut t, [_, _, _, notes, docs]) = tree();
    assert_eq!(t.remove(notes), 1);
    assert_eq!(t.len(), 4);
    assert_eq!(t.name(notes), None);
    assert_eq!(win(&t, notes), None);
    assert!(win(&t, docs).is_some());
    assert_eq!(t.remove(notes), 0);
}

#[test]
fn remove_a_folder_removes_its_subtree() {
    let (mut t, [root, users, ann, notes, docs]) = tree();
    assert_eq!(t.remove(ann), 3);
    assert_eq!(t.len(), 2);
    for gone in [ann, notes, docs] {
        assert_eq!(t.name(gone), None);
    }
    assert!(win(&t, users).is_some() && win(&t, root).is_some());
}

#[test]
fn remove_finds_descendants_pushed_before_the_folder() {
    let mut t = NameTable::new();
    let file = t.push(1, "a.txt", Flags::NONE).unwrap();
    let dir = t.push(NO_PARENT, "d", Flags::FOLDER).unwrap();
    assert_eq!(t.remove(dir), 2);
    assert_eq!(t.name(file), None);
}

#[test]
fn iteration_skips_removed_entries() {
    let (mut t, [_, _, _, notes, docs]) = tree();
    t.remove(docs);
    let ids: Vec<u32> = t.iter().map(|(id, _)| id).collect();
    assert_eq!(ids, vec![0, 1, 2, notes]);
    let ranged: Vec<u32> = t.iter_range(1..3).map(|(id, _)| id).collect();
    assert_eq!(ranged, vec![1, 2]);
    assert_eq!(t.iter_range(3..900).count(), 1);
}

#[test]
fn compact_drops_removed_rewrites_ids_and_keeps_paths() {
    let (mut t, [root, users, ann, notes, docs]) = tree();
    let extra = t.push(users, "bob", Flags::FOLDER).unwrap();
    t.rename(extra, users, "carol").unwrap();
    t.remove(ann);
    let map = t.compact();
    assert_eq!(map[root as usize], Some(0));
    assert_eq!(map[users as usize], Some(1));
    assert_eq!(map[ann as usize], None);
    assert_eq!(map[notes as usize], None);
    assert_eq!(map[docs as usize], None);
    let carol = map[extra as usize].unwrap();
    assert_eq!(win(&t, carol).as_deref(), Some("C:\\Users\\carol"));
    assert_eq!((t.len(), t.slots()), (3, 3));
    assert!(!t.names.contains("bob"));
}

#[test]
fn flags_round_trip_and_hide_the_deleted_bit() {
    let mut t = NameTable::new();
    let both = Flags::HIDDEN | Flags::SYSTEM;
    let id = t.push(NO_PARENT, "pagefile.sys", both).unwrap();
    assert_eq!(t.flags(id), Some(both));
    assert!(both.contains(Flags::HIDDEN) && !both.contains(Flags::FOLDER));
    t.remove(id);
    assert_eq!(t.flags(id), None);
}

#[test]
fn non_ascii_names_survive() {
    let mut t = NameTable::new();
    let root = t.push(NO_PARENT, "D:", Flags::FOLDER).unwrap();
    let dir = t.push(root, "Zażółć gęślą jaźń", Flags::FOLDER).unwrap();
    let file = t.push(dir, "日本語のファイル 🦀.txt", Flags::NONE).unwrap();
    assert_eq!(t.name(dir), Some("Zażółć gęślą jaźń"));
    assert_eq!(
        win(&t, file).as_deref(),
        Some("D:\\Zażółć gęślą jaźń\\日本語のファイル 🦀.txt")
    );
}

#[test]
fn very_long_names_are_kept_whole() {
    let mut t = NameTable::new();
    let long = "x".repeat(70_000);
    let a = t.push(NO_PARENT, &long, Flags::NONE).unwrap();
    let b = t.push(NO_PARENT, "after", Flags::NONE).unwrap();
    assert_eq!(t.name(a).map(str::len), Some(70_000));
    assert_eq!(t.name(b), Some("after"));
}

#[test]
fn a_nul_ends_the_name() {
    let mut t = NameTable::new();
    let a = t.push(NO_PARENT, "ab\0cd", Flags::NONE).unwrap();
    let b = t.push(NO_PARENT, "next", Flags::NONE).unwrap();
    assert_eq!(t.name(a), Some("ab"));
    assert_eq!(t.name(b), Some("next"));
}

#[test]
fn a_parent_cycle_made_with_late_parents_gives_no_path() {
    let mut t = NameTable::new();
    let a = t.push(1, "a", Flags::FOLDER).unwrap();
    let b = t.push(0, "b", Flags::FOLDER).unwrap();
    assert_eq!(t.path_with(a, '/'), None);
    assert_eq!(t.remove(b), 2);
    assert_eq!(t.remove(a), 0);
}

#[test]
fn scan_feeds_live_names_to_the_fuzzy_matcher() {
    let (mut t, [_, _, _, notes, docs]) = tree();
    let other = t.push(docs, "notebook.md", Flags::NONE).unwrap();
    let gone = t.push(docs, "notes-old.txt", Flags::NONE).unwrap();
    t.remove(gone);
    let mut fuzzy = Fuzzy::new("note", false);
    let hits: Vec<u32> = t
        .scan(0..t.slots(), &mut fuzzy)
        .map(|(id, m)| {
            assert_eq!(m.indices.len(), 4);
            id
        })
        .collect();
    assert_eq!(hits, vec![notes, other]);
}

fn synthetic(count: u32) -> NameTable {
    let mut t = NameTable::new();
    t.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    for i in 1..count {
        let name = format!("document-{i:07}.txt");
        let flags = if i % 8 == 0 {
            Flags::FOLDER
        } else {
            Flags::NONE
        };
        t.push((i - 1) / 8, &name, flags).unwrap();
    }
    t.shrink_to_fit();
    t
}

fn assert_budget(count: u32) {
    let t = synthetic(count);
    assert_eq!(t.len(), count as usize);
    let per_entry = t.memory_bytes() as f64 / t.len() as f64;
    use std::io::Write as _;
    writeln!(
        std::io::stdout(),
        "{count} entries: {per_entry:.6} bytes/entry"
    )
    .unwrap();
    assert!(per_entry <= 54.0, "{per_entry} bytes per entry");
    let last = count - 1;
    assert!(
        t.path_with(last, '\\')
            .is_some_and(|p| p.starts_with("C:\\"))
    );
}

#[test]
fn memory_per_entry_stays_within_budget_on_a_million_entries() {
    assert_budget(1_000_000);
}

#[test]
fn memory_per_entry_stays_within_budget_on_two_million_entries() {
    assert_budget(2_000_000);
}

#[test]
fn a_moved_entry_leaves_with_its_new_folder_and_not_its_old_one() {
    let (mut t, [root, users, ann, notes, docs]) = tree();
    t.rename(notes, docs, "notes.txt").unwrap();
    t.rename(docs, users, "docs").unwrap();
    assert_eq!(t.remove(ann), 1);
    assert_eq!(
        win(&t, notes).as_deref(),
        Some("C:\\Users\\docs\\notes.txt")
    );
    assert_eq!(t.remove(users), 3);
    assert_eq!(t.len(), 1);
    assert_eq!(t.name(root), Some("C:"));
}

#[test]
fn removal_finds_the_subtree_after_compaction_and_after_loading_raw_parts() {
    let (mut t, [_, users, _, notes, _]) = tree();
    t.remove(notes);
    let map = t.compact();
    let (names, offsets, parents, flags) = t.raw_parts();
    let mut loaded = NameTable::from_raw_parts(
        names.to_owned(),
        (offsets.to_vec(), parents.to_vec(), flags.to_vec()),
        (t.stored_totals().0.to_vec(), t.stored_totals().1.collect()),
    )
    .unwrap();
    let users = map[users as usize].unwrap();
    assert_eq!(t.remove(users), 3);
    assert_eq!(loaded.remove(users), 3);
    assert_eq!((t.len(), loaded.len()), (1, 1));
}
