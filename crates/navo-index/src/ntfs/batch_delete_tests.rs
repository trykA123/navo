//! Tests the journal batch rebuild signal for unsafe folder deletes.

use super::*;

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
