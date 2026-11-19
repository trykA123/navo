//! Tests the journal batch rebuild signal for unsafe folder deletes.

use super::*;

/// Preserves a blocked directory move when a carried rename emits a removal.
#[test]
fn rename_expiry_queues_the_remove_behind_the_blocked_folder_move() {
    let mut index = base();
    let original = view(&index);
    for age in 0..=crate::usn::MAX_CARRIED_READS {
        let entry = if age == 0 {
            record(20, ROOT, OLD_NAME, "trash")
        } else {
            record(30, ROOT, 0x4, "away")
        };
        index
            .apply_batch(UsnBatch {
                next_usn: 120 + i64::from(age),
                records: vec![entry],
            })
            .unwrap();
    }
    let unresolved = index
        .apply_batch(UsnBatch {
            next_usn: 200,
            records: vec![
                record(20, 99, NEW_NAME, "moved"),
                record(20, 99, OLD_NAME, "moved"),
            ],
        })
        .unwrap();
    assert_eq!(unresolved.len(), 2);
    assert!(matches!(
        unresolved[0].change,
        TableChange::Rename { file_id: 20, .. }
    ));
    assert_eq!(unresolved[1], unresolved_folder_delete(99));
    assert_eq!(view(&index), original);
}

/// Uses the actual child links even after attributes clear the folder flag.
#[test]
fn cleared_folder_flag_cannot_hide_a_blocked_descendant_from_a_delete() {
    let mut index = base();
    let mut attributes = record(20, ROOT, 0x4, "trash");
    attributes.flags = Flags::NONE;
    let unresolved = index
        .apply_batch(UsnBatch {
            next_usn: 200,
            records: vec![
                attributes,
                record(21, 20, OLD_NAME, "deep"),
                record(21, 99, NEW_NAME, "deep"),
                record(20, ROOT, DELETE, "trash"),
                record(99, ROOT, CREATE, "late"),
            ],
        })
        .unwrap();
    assert_eq!(unresolved, [unresolved_folder_delete(99)]);
    assert_eq!(
        index.table.path_with(index.ids[&22], '\\').as_deref(),
        Some("C:\\late\\deep\\b.txt")
    );
}

/// Forces a rebuild when unrelated blocked moves exhaust the ancestry budget.
#[test]
fn delete_reports_unresolved_when_ancestor_steps_exhaust_the_scan_budget() {
    let mut index = base();
    let mut parent = ROOT;
    let mut records = Vec::new();
    for folder in 100..180 {
        records.push(record(folder, parent, CREATE, "nested"));
        parent = folder;
    }
    records.extend((200..1_100).map(|file| record(file, parent, CREATE, "file.txt")));
    index
        .apply_batch(UsnBatch {
            next_usn: 150,
            records,
        })
        .unwrap();
    let mut records: Vec<_> = (200..1_100)
        .map(|file| record(file, 99, CREATE, "moved.txt"))
        .collect();
    records.extend([
        record(20, ROOT, DELETE, "trash"),
        record(99, ROOT, CREATE, "late"),
    ]);
    let unresolved = index
        .apply_batch(UsnBatch {
            next_usn: 200,
            records,
        })
        .unwrap();
    assert_eq!(unresolved, [unresolved_folder_delete(99)]);
    assert!(index.ids.contains_key(&22));
}

/// Keeps a replacement and its child away from the old subtree while a rebuild is needed.
#[test]
fn an_unresolved_delete_rejects_a_replacement_folder_and_child() {
    let mut index = base();
    let before = view(&index);
    let unresolved = index
        .apply_batch(UsnBatch {
            next_usn: 200,
            records: vec![
                record(21, 20, OLD_NAME, "deep"),
                record(21, 99, NEW_NAME, "deep"),
                record(20, ROOT, DELETE, "trash"),
                record(20, ROOT, CREATE, "replacement"),
                record(20, ROOT, 0x4, "replacement"),
                record(90, 20, CREATE, "child.txt"),
            ],
        })
        .unwrap();
    assert_eq!(view(&index), before);
    assert_eq!(unresolved.len(), 5);
    assert_eq!(unresolved[0], unresolved_folder_delete(99));
    for item in &unresolved[1..4] {
        assert_eq!(item.missing_reference, 99);
    }
    assert!(matches!(
        unresolved[1].change,
        TableChange::Add { file_id: 20, .. }
    ));
    assert!(matches!(
        unresolved[2].change,
        TableChange::Attributes { file_id: 20, .. }
    ));
    assert!(matches!(
        unresolved[3].change,
        TableChange::Add { file_id: 90, .. }
    ));
}

/// Checks that the conservative cutoff applies only above 4,096 blocked files.
#[test]
fn folder_deletes_report_unresolved_above_the_blocked_scan_limit() {
    for count in [4_096, 4_097] {
        let mut index = base();
        let mut records: Vec<_> = (100..100 + count)
            .map(|file| record(file, 99, CREATE, "waiting.txt"))
            .collect();
        records.extend([
            record(20, ROOT, DELETE, "trash"),
            record(99, ROOT, CREATE, "late"),
        ]);
        let unresolved = index
            .apply_batch(UsnBatch {
                next_usn: 200,
                records,
            })
            .unwrap();
        if count == 4_097 {
            assert_eq!(unresolved, [unresolved_folder_delete(99)]);
            assert!(index.ids.contains_key(&22));
        } else {
            assert!(unresolved.is_empty());
            assert!(!index.ids.contains_key(&20));
        }
    }
}
