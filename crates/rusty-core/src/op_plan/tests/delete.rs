use super::*;

fn tree() -> Vec<ScannedItem> {
    vec![
        folder(r"C:\a\b", 0),
        folder(r"C:\a\b\c", 1),
        file(r"C:\a\b\c\d.txt", 2, 3),
        link(r"C:\a\b\l", 1),
        file(r"C:\a\b\e.txt", 1, 4),
        file(r"C:\a\f.txt", 0, 5),
    ]
}

fn recycle() -> OperationPlan {
    plan(&request(OpKind::Recycle, tree(), None))
}

fn delete() -> OperationPlan {
    plan(&request(OpKind::Delete, tree(), None))
}

#[test]
fn recycle_has_one_step_per_selected_item() {
    assert_eq!(
        recycle().steps,
        vec![
            Step::Recycle {
                location: loc(r"C:\a\b"),
            },
            Step::Recycle {
                location: loc(r"C:\a\f.txt"),
            },
        ]
    );
}

#[test]
fn recycle_counts_the_scanned_bytes() {
    assert_eq!(recycle().totals.bytes, 12);
}

#[test]
fn recycle_counts_the_scanned_items() {
    let totals = recycle().totals;
    assert_eq!((totals.files, totals.folders), (3, 2));
}

#[test]
fn recycle_is_not_permanent() {
    assert!(!recycle().risks.contains(&Risk::Permanent));
}

#[test]
fn delete_removes_every_item_in_reverse_scan_order() {
    assert_eq!(
        delete().steps,
        vec![
            remove_file(r"C:\a\f.txt"),
            remove_file(r"C:\a\b\e.txt"),
            remove_link(r"C:\a\b\l"),
            remove_file(r"C:\a\b\c\d.txt"),
            remove_folder(r"C:\a\b\c"),
            remove_folder(r"C:\a\b"),
        ]
    );
}

#[test]
fn delete_is_permanent() {
    assert!(delete().risks.contains(&Risk::Permanent));
}

#[test]
fn delete_of_one_small_file_is_still_permanent() {
    let plan = plan(&request(
        OpKind::Delete,
        vec![file(r"C:\a\x.txt", 0, 1)],
        None,
    ));
    assert_eq!(plan.risks, vec![Risk::Permanent]);
}

#[test]
fn delete_totals_removed_files_folders_and_scanned_bytes() {
    assert_eq!(
        delete().totals,
        Totals {
            files: 3,
            folders: 2,
            bytes: 12
        }
    );
}
