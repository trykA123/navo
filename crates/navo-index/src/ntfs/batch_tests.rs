use super::*;
use crate::table::Totals;
use crate::usn::UsnRecord;

const ROOT: u128 = (5 << 48) | 5;
const CREATE: u32 = 0x100;
const DELETE: u32 = 0x200;
const OLD_NAME: u32 = 0x1000;
const NEW_NAME: u32 = 0x2000;
const POSITION: JournalPosition = JournalPosition {
    journal_id: 7,
    next_usn: 100,
};

fn record(file_id: u128, parent_id: u128, reason: u32, name: &str) -> UsnRecord {
    UsnRecord {
        size: None,
        file_id,
        parent_id,
        usn: 100,
        reason,
        flags: if name.contains('.') {
            Flags::NONE
        } else {
            Flags::FOLDER
        },
        name: name.into(),
    }
}

fn sized(file_id: u128, parent_id: u128, name: &str, bytes: u64) -> UsnRecord {
    UsnRecord {
        size: Some(bytes),
        ..record(file_id, parent_id, CREATE, name)
    }
}

fn base() -> NtfsIndex {
    let mut index = NtfsIndex::new('C', POSITION).unwrap();
    index
        .apply_batch(UsnBatch {
            next_usn: 110,
            records: vec![
                record(10, ROOT, CREATE, "docs"),
                record(11, 10, CREATE, "old"),
                sized(12, 11, "a.txt", 3),
                record(20, ROOT, CREATE, "trash"),
                record(21, 20, CREATE, "deep"),
                sized(22, 21, "b.txt", 5),
                sized(23, 20, "c.txt", 7),
                record(30, ROOT, CREATE, "away"),
            ],
        })
        .unwrap();
    index
}

fn mixed_batch() -> UsnBatch {
    let mut attributes = record(12, 11, 0x4, "a.txt");
    attributes.flags = Flags::HIDDEN;
    UsnBatch {
        next_usn: 200,
        records: vec![
            sized(41, 40, "early.txt", 11),
            record(40, ROOT, CREATE, "late"),
            record(11, 10, OLD_NAME, "old"),
            record(11, 40, NEW_NAME, "moved"),
            sized(42, 11, "new.txt", 13),
            record(20, ROOT, DELETE, "trash"),
            attributes,
            record(99, 10, 0x4, "ghost.txt"),
            record(30, ROOT, OLD_NAME, "away"),
            sized(43, ROOT, "last.txt", 17),
        ],
    }
}

type Row = (
    u128,
    Option<String>,
    Option<Flags>,
    Option<u64>,
    Option<Totals>,
);

fn view(index: &NtfsIndex) -> Vec<Row> {
    let mut rows: Vec<_> = index
        .ids
        .iter()
        .map(|(file_id, id)| {
            (
                file_id,
                index.table.path_with(id, '\\'),
                index.table.flags(id),
                index.table.size(id),
                index.table.totals(id),
            )
        })
        .collect();
    rows.sort_by_key(|row| row.0);
    rows
}

fn stepped(limit: usize) -> (NtfsIndex, Vec<UnresolvedChange>, usize) {
    let mut index = base();
    let mut applying = index.begin_batch(mixed_batch()).unwrap();
    let mut steps = 1;
    while applying.step(&mut index, limit).unwrap() {
        steps += 1;
        assert_consistent(&index);
    }
    let unresolved = applying.finish(&mut index).unwrap();
    (index, unresolved, steps)
}

fn assert_consistent(index: &NtfsIndex) {
    assert!(index.ids.values().all(|&id| index.table.name(id).is_some()));
    assert!(index.table.valid_structure());
    assert_eq!(
        index.position,
        JournalPosition {
            next_usn: 110,
            ..POSITION
        }
    );
    for (file_id, bytes) in [(41, 11), (42, 13), (43, 17), (12, 3)] {
        if let Some(&id) = index.ids.get(&file_id) {
            assert_eq!(index.table.size(id), Some(bytes), "{file_id}");
        }
    }
}

#[test]
fn stepwise_application_matches_one_batch_at_every_chunk_size() {
    let mut whole = base();
    let expected_unresolved = whole.apply_batch(mixed_batch()).unwrap();
    assert_eq!(expected_unresolved.len(), 1);
    assert_eq!(
        whole.table.path_with(whole.ids[&11], '\\').as_deref(),
        Some("C:\\late\\moved")
    );
    assert!(!whole.ids.contains_key(&22));
    for limit in 1..=16 {
        let (index, unresolved, steps) = stepped(limit);
        assert!(limit > 1 || steps >= 8, "limit {limit} took {steps} steps");
        assert_eq!(view(&index), view(&whole), "limit {limit}");
        assert_eq!(index.position, whole.position);
        assert_eq!(index.pending_renames(), whole.pending_renames());
        assert_eq!(unresolved, expected_unresolved);
        assert_eq!(index.table.len(), whole.table.len());
    }
}

#[test]
fn a_rename_is_never_half_applied_between_steps() {
    let mut index = base();
    let mut applying = index.begin_batch(mixed_batch()).unwrap();
    let mut seen = Vec::new();
    loop {
        let path = index.table.path_with(index.ids[&11], '\\').unwrap();
        assert!(
            path == "C:\\docs\\old" || path == "C:\\late\\moved",
            "half-applied path {path}"
        );
        assert_eq!(
            index.table.path_with(index.ids[&12], '\\').unwrap(),
            format!("{path}\\a.txt")
        );
        seen.push(path);
        if !applying.step(&mut index, 1).unwrap() {
            break;
        }
    }
    applying.finish(&mut index).unwrap();
    assert!(seen.iter().any(|path| path == "C:\\docs\\old"));
    assert!(seen.iter().any(|path| path == "C:\\late\\moved"));
}

#[test]
fn a_cyclic_move_does_not_stop_later_steps() {
    let mut index = base();
    let mut applying = index
        .begin_batch(UsnBatch {
            next_usn: 200,
            records: vec![
                record(20, ROOT, DELETE, "trash"),
                record(10, ROOT, OLD_NAME, "docs"),
                record(10, 11, NEW_NAME, "docs"),
                sized(43, ROOT, "never.txt", 1),
            ],
        })
        .unwrap();
    while applying.step(&mut index, 1).unwrap() {
        assert_consistent(&index);
    }
    assert!(applying.finish(&mut index).unwrap().is_empty());
    assert_eq!(index.position.next_usn, 200);
    assert!(!index.ids.contains_key(&22));
    assert!(index.ids.contains_key(&43));
    assert!(index.ids.values().all(|&id| index.table.name(id).is_some()));
}

#[test]
fn a_stale_cursor_is_refused_before_any_change() {
    let index = base();
    let before = view(&index);
    assert!(matches!(
        index.begin_batch(UsnBatch {
            next_usn: 110,
            records: vec![record(50, ROOT, CREATE, "x")],
        }),
        Err(NtfsError::InvalidResponse(_))
    ));
    assert_eq!(view(&index), before);
    assert!(index.pending_renames().is_empty());
}

#[test]
fn a_prepared_batch_changes_nothing_until_its_first_step() {
    let mut index = base();
    let before = view(&index);
    let mut applying = index.begin_batch(mixed_batch()).unwrap();
    assert_eq!(view(&index), before);
    assert!(index.pending_renames().is_empty());
    applying.step(&mut index, 1).unwrap();
    assert_eq!(index.pending_renames().len(), 1);
}

#[test]
fn a_prepared_batch_refuses_an_index_at_another_position() {
    let index = base();
    let mut applying = index.begin_batch(mixed_batch()).unwrap();
    let mut moved = base();
    moved.restart_at(JournalPosition {
        journal_id: 8,
        next_usn: 0,
    });
    let before = view(&moved);
    assert!(matches!(
        applying.step(&mut moved, 1),
        Err(NtfsError::InvalidResponse(_))
    ));
    assert_eq!(view(&moved), before);
    assert!(moved.pending_renames().is_empty());
}

/// Checks that deleting an unseen file clears its blocked creation.
#[test]
fn delete_cancels_a_blocked_add() {
    let mut index = NtfsIndex::new('C', POSITION).unwrap();
    let unresolved = index
        .apply_batch(UsnBatch {
            next_usn: 200,
            records: vec![
                record(30, 99, CREATE, "gone.txt"),
                record(30, 99, DELETE, "gone.txt"),
            ],
        })
        .unwrap();
    assert_eq!(unresolved.len(), 0);
}

/// Checks that deleting an indexed file clears its blocked rename and removes it.
#[test]
fn delete_cancels_a_blocked_rename_and_removes_the_indexed_entry() {
    let mut index = base();
    let unresolved = index
        .apply_batch(UsnBatch {
            next_usn: 200,
            records: vec![
                record(21, 20, OLD_NAME, "deep"),
                record(21, 99, NEW_NAME, "deep"),
                record(21, 99, DELETE, "deep"),
            ],
        })
        .unwrap();
    assert!(unresolved.is_empty(), "{unresolved:?}");
    assert!(!index.ids.contains_key(&21));
}

/// Checks that a second creation of the same reference still waits for its parent.
#[test]
fn a_blocked_add_after_delete_starts_a_new_chain() {
    let mut index = NtfsIndex::new('C', POSITION).unwrap();
    let unresolved = index
        .apply_batch(UsnBatch {
            next_usn: 200,
            records: vec![
                record(30, 99, CREATE, "gone.txt"),
                record(30, 99, DELETE, "gone.txt"),
                record(30, 99, CREATE, "new.txt"),
            ],
        })
        .unwrap();
    assert_eq!(
        unresolved,
        [UnresolvedChange {
            missing_reference: 99,
            change: TableChange::Add {
                file_id: 30,
                parent_id: 99,
                name: "new.txt".into(),
                flags: Flags::NONE,
            },
        }]
    );
}

/// Checks that a late parent adds only its own slot after the child was deleted.
#[test]
fn a_late_parent_never_materializes_a_cancelled_add() {
    let mut index = NtfsIndex::new('C', POSITION).unwrap();
    let slots = index.table.slots();
    let unresolved = index
        .apply_batch(UsnBatch {
            next_usn: 200,
            records: vec![
                record(30, 99, CREATE, "gone.txt"),
                record(30, 99, DELETE, "gone.txt"),
                record(99, ROOT, CREATE, "late"),
            ],
        })
        .unwrap();
    assert!(unresolved.is_empty(), "{unresolved:?}");
    assert!(index.ids.contains_key(&99));
    assert!(!index.ids.contains_key(&30));
    assert_eq!(index.table.slots(), slots + 1);
}

/// Replays a child move out of a folder that is deleted before the destination arrives.
fn assert_subtree_survives_folder_delete(move_deleted_folder: bool) {
    let mut index = base();
    let mut records = vec![
        record(21, 20, OLD_NAME, "deep"),
        record(21, 99, NEW_NAME, "deep"),
    ];
    if move_deleted_folder {
        records.extend([
            record(20, ROOT, OLD_NAME, "trash"),
            record(20, 99, NEW_NAME, "trash"),
        ]);
    }
    records.extend([
        record(20, 99, DELETE, "trash"),
        record(99, ROOT, CREATE, "late"),
    ]);
    let unresolved = index
        .apply_batch(UsnBatch {
            next_usn: 200,
            records,
        })
        .unwrap();
    assert_eq!(unresolved, [unresolved_folder_delete(99)]);
    assert_eq!(
        index
            .ids
            .get(&22)
            .and_then(|&id| index.table.path_with(id, '\\'))
            .as_deref(),
        Some("C:\\late\\deep\\b.txt")
    );
    let folder = if move_deleted_folder {
        "C:\\late\\trash"
    } else {
        "C:\\trash"
    };
    assert_eq!(
        index.table.path_with(index.ids[&20], '\\').as_deref(),
        Some(folder)
    );
}

/// Builds the reported delete for the indexed folder whose child waits for `missing`.
fn unresolved_folder_delete(missing: u128) -> UnresolvedChange {
    UnresolvedChange {
        missing_reference: missing,
        change: TableChange::Remove { file_id: 20 },
    }
}

/// Checks a delete when both the folder and the escaping child have blocked moves.
#[test]
fn a_blocked_folder_delete_preserves_its_escaping_subtree() {
    assert_subtree_survives_folder_delete(true);
}

/// Checks a delete when only the escaping child has a blocked move.
#[test]
fn an_unblocked_folder_delete_preserves_its_escaping_subtree() {
    assert_subtree_survives_folder_delete(false);
}

/// Checks that waking a cancelled slot leaves the replacement folder and child blocked.
#[test]
fn a_cancelled_waiter_cannot_unblock_a_replacement_folder() {
    let mut index = NtfsIndex::new('C', POSITION).unwrap();
    let mut attributes = record(30, 98, 0x4, "folder");
    attributes.flags = Flags::FOLDER | Flags::HIDDEN;
    let records = vec![
        record(30, 99, CREATE, "folder"),
        record(30, 99, DELETE, "folder"),
        record(30, 98, CREATE, "folder"),
        record(31, 30, CREATE, "child.txt"),
        record(99, ROOT, CREATE, "first"),
        attributes,
        record(98, ROOT, CREATE, "second"),
    ];
    let mut applying = index
        .begin_batch(UsnBatch {
            next_usn: 200,
            records,
        })
        .unwrap();
    for _ in 0..6 {
        assert!(applying.step(&mut index, 1).unwrap());
    }
    assert!(index.ids.contains_key(&99));
    assert!(!index.ids.contains_key(&30));
    assert!(!index.ids.contains_key(&31));
    let unresolved = applying.finish(&mut index).unwrap();
    assert!(unresolved.is_empty(), "{unresolved:?}");
    assert_eq!(
        index.table.flags(index.ids[&30]),
        Some(Flags::FOLDER | Flags::HIDDEN)
    );
    assert_eq!(
        index.table.path_with(index.ids[&31], '\\').as_deref(),
        Some("C:\\second\\folder\\child.txt")
    );
}

#[path = "batch_delete_tests.rs"]
mod batch_delete_tests;
