//! AF9d: journal changes inside NTFS metadata folders under `$Extend` (`$Deleted` above all)
//! never ask for a rebuild; a file Windows parks in `$Deleted` leaves the index.

use super::test_support::extend_folder_build;
use super::{NtfsIndex, Pending, UnresolvedChange};
use crate::table::{Flags, TableChange};
use crate::usn::{UsnBatch, UsnRecord};

const DATA_EXTEND: u32 = 0x2;
const CREATE: u32 = 0x100;
const OLD_NAME: u32 = 0x1000;
const NEW_NAME: u32 = 0x2000;
const EXTEND: u128 = (11 << 48) | 11;
const DELETED: u128 = (1 << 48) | 0x1D;
const TXF: u128 = (1 << 48) | 28;
const DOCS: u128 = 30;
const FILE: u128 = 21;
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

fn folder(file_id: u128, parent_id: u128, reason: u32, name: &str) -> UsnRecord {
    UsnRecord {
        flags: Flags::FOLDER,
        ..record(file_id, parent_id, reason, name)
    }
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
            !matches!(
                item.change,
                TableChange::Size { .. } | TableChange::Attributes { .. }
            )
        })
        .collect()
}

fn indexed_name(index: &NtfsIndex, file_id: u128) -> Option<&str> {
    index.ids.get(&file_id).and_then(|&id| index.table.name(id))
}

/// The VM pattern: an open file's last link is deleted, Windows renames it into `$Deleted` and
/// it shrinks there. The paired new name removes the indexed entry; an unpaired one may be another
/// hard link leaving, so it stays blocked and the entry stays (AF9h).
#[test]
fn a_file_moved_into_deleted_leaves_the_index_and_its_size_change_needs_no_rebuild() {
    for paired in [true, false] {
        let mut index = extend_folder_build().index;
        assert_eq!(indexed_name(&index, FILE), Some("x.txt"));
        let mut records = vec![record(FILE, DELETED, NEW_NAME, "$X0000000000000015")];
        if paired {
            records.insert(0, record(FILE, DOCS, OLD_NAME, "x.txt"));
        }
        records.push(UsnRecord {
            size: Some(0),
            ..record(FILE, DELETED, DATA_EXTEND, "$X0000000000000015")
        });
        let unresolved = apply(&mut index, 120, records);
        if paired {
            assert!(blocking(&unresolved).is_empty(), "{unresolved:?}");
            assert_eq!(indexed_name(&index, FILE), None);
        } else {
            assert!(!blocking(&unresolved).is_empty(), "{unresolved:?}");
            assert_eq!(indexed_name(&index, FILE), Some("x.txt"));
        }
    }
}

/// Creates inside `$Deleted`, inside a folder created there, inside a nested metadata folder and
/// directly under `$Extend` are ignored; a file renamed out of `$Deleted` joins the index.
#[test]
fn creates_inside_metadata_folders_and_a_move_out_of_deleted_need_no_rebuild() {
    let mut index = extend_folder_build().index;
    let entries = index.table.len();
    let records = vec![
        record(60, DELETED, CREATE, "$X60"),
        folder(61, DELETED, CREATE, "$X61"),
        record(62, 61, CREATE, "inner"),
        record(63, TXF, CREATE, "log.blf"),
        record(64, EXTEND, CREATE, "$Repair"),
        record(65, DELETED, OLD_NAME, "$X65"),
        record(65, DOCS, NEW_NAME, "back.txt"),
    ];
    let unresolved = apply(&mut index, 120, records);
    assert!(blocking(&unresolved).is_empty(), "{unresolved:?}");
    assert_eq!(index.table.len(), entries + 1);
    assert_eq!(indexed_name(&index, 65), Some("back.txt"));
    for file_id in [60, 61, 62, 63, 64] {
        assert_eq!(indexed_name(&index, file_id), None, "file {file_id}");
    }
}

/// An indexed folder moved into `$Deleted` leaves with its subtree; a create in its former
/// subfolder is then ignored instead of asking for a rebuild.
#[test]
fn a_folder_moved_into_deleted_leaves_with_its_subtree() {
    let mut index = extend_folder_build().index;
    let records = vec![
        folder(DOCS, 5, OLD_NAME, "docs"),
        folder(DOCS, DELETED, NEW_NAME, "$X30"),
    ];
    assert!(blocking(&apply(&mut index, 120, records)).is_empty());
    assert_eq!(indexed_name(&index, DOCS), None);
    assert_eq!(indexed_name(&index, FILE), None);
    let unresolved = apply(&mut index, 130, vec![record(70, SUB, CREATE, "late.txt")]);
    assert!(blocking(&unresolved).is_empty(), "{unresolved:?}");
}

/// AF9e: a rename into `$Deleted` of a file the index lacks (its old name sat in an orphan folder
/// or in `$RmMetadata`) is ignored, though the table misses the file before it checks the parent.
#[test]
fn an_unknown_file_renamed_into_deleted_needs_no_rebuild() {
    for old_parent in [99, TXF] {
        let mut index = extend_folder_build().index;
        let entries = index.table.len();
        let records = vec![
            record(80, old_parent, OLD_NAME, "tmp"),
            record(80, DELETED, NEW_NAME, "$X80"),
        ];
        let unresolved = apply(&mut index, 120, records);
        assert!(
            blocking(&unresolved).is_empty(),
            "old parent {old_parent}: {unresolved:?}"
        );
        assert_eq!(index.table.len(), entries);
    }
    // The batch turns a rename of an unknown file into an add; the resolver alone ignores it too.
    let mut index = extend_folder_build().index;
    let rename = TableChange::Rename {
        file_id: 80,
        parent_id: DELETED,
        name: "$X80".into(),
        flags: Flags::NONE,
    };
    let taken = Pending::default().apply_tracked(&mut index, &rename);
    assert!(matches!(taken, Ok(false)), "{taken:?}");
}

/// AF9e: a POSIX delete of a hard link the index does not show parks that link in `$Deleted`;
/// the file keeps its indexed name. A folder whose paired old link is not the indexed one blocks.
#[test]
fn a_departing_other_hard_link_keeps_the_indexed_file() {
    let mut index = extend_folder_build().index;
    let records = vec![
        record(FILE, SUB, OLD_NAME, "b.txt"),
        record(FILE, DELETED, NEW_NAME, "$X15"),
    ];
    let unresolved = apply(&mut index, 120, records);
    assert!(blocking(&unresolved).is_empty(), "{unresolved:?}");
    assert_eq!(indexed_name(&index, FILE), Some("x.txt"));
    let records = vec![
        folder(SUB, 5, OLD_NAME, "elsewhere"),
        folder(SUB, DELETED, NEW_NAME, "$X20"),
    ];
    let unresolved = apply(&mut index, 130, records);
    assert!(
        !blocking(&unresolved).is_empty(),
        "a folder link mismatch must block"
    );
    assert_eq!(indexed_name(&index, SUB), Some("sub"));
}
