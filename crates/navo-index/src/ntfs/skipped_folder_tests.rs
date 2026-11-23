//! AF9: journal changes into a folder the build skipped resolve without asking for a rebuild.

use super::test_support::skipped_folder_build;
use super::{NtfsIndex, UnresolvedChange};
use crate::table::{Flags, TableChange};
use crate::usn::{UsnBatch, UsnRecord};

const CREATE: u32 = 0x100;
const DELETE: u32 = 0x200;
const OLD_NAME: u32 = 0x1000;
const NEW_NAME: u32 = 0x2000;
const ORPHAN: u128 = 8;
const DOCS: u128 = 30;
const FILE: u128 = 21;
const ROOT: u128 = 5;
const SUB: u128 = 32;

fn record(file_id: u128, parent_id: u128, reason: u32, name: &str) -> UsnRecord {
    UsnRecord {
        file_id,
        parent_id,
        usn: 110,
        reason,
        flags: Flags::NONE,
        name: name.into(),
        size: None,
    }
}

/// Moves `C:\docs\x.txt` (single link) into `parent` under the same name.
fn move_file(parent: u128) -> Vec<UsnRecord> {
    vec![
        record(FILE, DOCS, OLD_NAME, "x.txt"),
        record(FILE, parent, NEW_NAME, "x.txt"),
    ]
}

/// Moves folder `C:\docs` (holding `x.txt` and `sub`) from `from` to `to` under the same name.
fn move_folder(from: u128, to: u128) -> Vec<UsnRecord> {
    let folder = |parent, reason| UsnRecord {
        flags: Flags::FOLDER,
        ..record(DOCS, parent, reason, "docs")
    };
    vec![folder(from, OLD_NAME), folder(to, NEW_NAME)]
}

fn apply(index: &mut NtfsIndex, next_usn: i64, records: Vec<UsnRecord>) -> Vec<UnresolvedChange> {
    let batch = UsnBatch { next_usn, records };
    index.apply_batch(batch).expect("apply")
}

/// Keeps only the changes that would ask the service for a rebuild.
fn blocking(unresolved: &[UnresolvedChange]) -> Vec<&UnresolvedChange> {
    unresolved
        .iter()
        .filter(|item| {
            matches!(
                item.change,
                TableChange::Add { .. } | TableChange::Rename { .. }
            )
        })
        .collect()
}

fn indexed_name(index: &NtfsIndex, file_id: u128) -> Option<&str> {
    index.ids.get(&file_id).and_then(|&id| index.table.name(id))
}

/// Missing references of the changes that would ask the service for a rebuild.
fn blocked_refs(unresolved: &[UnresolvedChange]) -> Vec<u128> {
    blocking(unresolved)
        .iter()
        .map(|item| item.missing_reference)
        .collect()
}

/// The journal cannot tell whether `x.txt` has a second link under an indexed folder, so the
/// indexed link stays and the move waits for a rebuild instead of hiding a reachable file.
#[test]
fn a_file_that_may_have_other_links_moved_into_a_skipped_folder_stays_unresolved() {
    let mut index = skipped_folder_build().index;
    assert_eq!(indexed_name(&index, FILE), Some("x.txt"));
    let unresolved = apply(&mut index, 120, move_file(ORPHAN));
    assert_eq!(blocked_refs(&unresolved), [ORPHAN]);
    assert_eq!(indexed_name(&index, FILE), Some("x.txt"));
}

/// A hard-link refresh or an unpaired new name upserts an indexed file into a skipped folder;
/// ignoring it would keep a stale path, so it waits for a rebuild.
#[test]
fn an_add_of_an_indexed_file_into_a_skipped_folder_stays_unresolved() {
    let mut index = skipped_folder_build().index;
    let unresolved = apply(&mut index, 120, vec![record(FILE, ORPHAN, CREATE, "x.txt")]);
    assert_eq!(blocked_refs(&unresolved), [ORPHAN]);
    assert_eq!(indexed_name(&index, FILE), Some("x.txt"));
}

/// Folders have one link, so an indexed folder moved into a skipped folder leaves with its subtree.
#[test]
fn an_indexed_folder_moved_into_a_skipped_folder_leaves_with_its_children() {
    let mut index = skipped_folder_build().index;
    let unresolved = apply(&mut index, 120, move_folder(ROOT, ORPHAN));
    assert!(unresolved.is_empty(), "{unresolved:?}");
    assert_eq!(indexed_name(&index, DOCS), None);
    assert!(
        !index.ids.contains_key(&FILE),
        "the child's reference is forgotten"
    );
    assert!(index.skipped_folders.contains(&DOCS));
}

/// Moved back, the folder's children were never journaled: the batch asks for a rebuild.
#[test]
fn a_folder_moved_out_and_back_reports_its_missing_children() {
    let mut index = skipped_folder_build().index;
    assert!(apply(&mut index, 120, move_folder(ROOT, ORPHAN)).is_empty());
    let unresolved = apply(&mut index, 130, move_folder(ORPHAN, ROOT));
    let reported: Vec<_> = unresolved
        .iter()
        .map(|item| item.missing_reference)
        .collect();
    assert_eq!(reported, [DOCS], "{unresolved:?}");
    assert!(!index.skipped_folders.contains(&DOCS));
}

#[test]
fn a_deleted_skipped_folder_leaves_the_set() {
    let mut index = skipped_folder_build().index;
    assert!(index.skipped_folders.contains(&ORPHAN));
    apply(&mut index, 120, vec![record(ORPHAN, 99, DELETE, "orphan")]);
    assert!(!index.skipped_folders.contains(&ORPHAN));
}

#[test]
fn an_add_into_a_skipped_folder_is_ignored_with_the_folders_it_creates() {
    let mut index = skipped_folder_build().index;
    let entries = index.table.len();
    let records = vec![
        UsnRecord {
            flags: Flags::FOLDER,
            ..record(40, ORPHAN, CREATE, "sub")
        },
        record(41, 40, CREATE, "new.txt"),
    ];
    let unresolved = apply(&mut index, 120, records);
    assert!(blocking(&unresolved).is_empty(), "{unresolved:?}");
    assert_eq!(index.table.len(), entries);
}

#[test]
fn a_move_into_a_parent_the_build_did_not_skip_stays_unresolved() {
    let mut index = skipped_folder_build().index;
    let unresolved = apply(&mut index, 120, move_file(77));
    let blocked: Vec<_> = blocking(&unresolved)
        .iter()
        .map(|item| item.missing_reference)
        .collect();
    assert_eq!(blocked, [77]);
    assert_eq!(indexed_name(&index, FILE), Some("x.txt"));
}

/// The old name came in an earlier read, so which link moved is unknown: nothing is dropped.
#[test]
fn a_move_whose_old_name_came_in_an_earlier_batch_stays_unresolved() {
    let mut index = skipped_folder_build().index;
    let [old, new] = <[UsnRecord; 2]>::try_from(move_file(ORPHAN)).expect("two records");
    assert!(apply(&mut index, 120, vec![old]).is_empty());
    let unresolved = apply(&mut index, 130, vec![new]);
    let blocked: Vec<_> = blocking(&unresolved)
        .iter()
        .map(|item| item.missing_reference)
        .collect();
    assert_eq!(blocked, [ORPHAN]);
    assert_eq!(indexed_name(&index, FILE), Some("x.txt"));
}

/// Another hard link of `x.txt` moved into a skipped folder; the indexed link stays as it is.
#[test]
fn another_link_of_a_file_moved_into_a_skipped_folder_is_ignored() {
    let mut index = skipped_folder_build().index;
    let records = vec![
        record(FILE, ROOT, OLD_NAME, "y.txt"),
        record(FILE, ORPHAN, NEW_NAME, "y.txt"),
    ];
    let unresolved = apply(&mut index, 120, records);
    assert!(blocking(&unresolved).is_empty(), "{unresolved:?}");
    assert_eq!(indexed_name(&index, FILE), Some("x.txt"));
}

/// A folder has one link, so an old name that differs from the indexed one is out of step:
/// the move waits for a rebuild.
#[test]
fn another_link_of_a_folder_moved_into_a_skipped_folder_stays_unresolved() {
    let mut index = skipped_folder_build().index;
    let folder = |parent, reason, name| UsnRecord {
        flags: Flags::FOLDER,
        ..record(DOCS, parent, reason, name)
    };
    let records = vec![
        folder(ROOT, OLD_NAME, "other"),
        folder(ORPHAN, NEW_NAME, "other"),
    ];
    let unresolved = apply(&mut index, 120, records);
    assert_eq!(blocked_refs(&unresolved), [ORPHAN]);
    assert_eq!(indexed_name(&index, DOCS), Some("docs"));
}

/// The moved folder's indexed subfolders join the skipped set too, so a later create inside
/// one of them resolves instead of asking for a rebuild.
#[test]
fn a_create_inside_a_subfolder_of_a_moved_folder_is_ignored() {
    let mut index = skipped_folder_build().index;
    assert_eq!(indexed_name(&index, SUB), Some("sub"));
    assert!(apply(&mut index, 120, move_folder(ROOT, ORPHAN)).is_empty());
    let unresolved = apply(&mut index, 130, vec![record(50, SUB, CREATE, "new.txt")]);
    assert!(blocking(&unresolved).is_empty(), "{unresolved:?}");
    assert!(index.skipped_folders.contains(&SUB));
}
