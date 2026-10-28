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
        .map(|(&file_id, &id)| {
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
fn a_failed_step_keeps_the_checkpoint_and_live_references() {
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
    let failed = loop {
        match applying.step(&mut index, 1) {
            Ok(true) => assert_consistent(&index),
            Ok(false) => panic!("the cycle must fail"),
            Err(error) => break error,
        }
    };
    assert!(matches!(failed, NtfsError::Table(TableError::Cycle(_))));
    assert_eq!(
        index.position,
        JournalPosition {
            next_usn: 110,
            ..POSITION
        }
    );
    assert!(!index.ids.contains_key(&22));
    assert!(!index.ids.contains_key(&43));
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
