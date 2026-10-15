use super::*;

fn move_into(items: Vec<ScannedItem>, target: &str, same_volume: bool) -> PlanRequest {
    PlanRequest {
        same_volume,
        ..request(OpKind::Move, items, Some(target))
    }
}

fn tree() -> Vec<ScannedItem> {
    vec![
        folder(r"C:\a\b", 0),
        folder(r"C:\a\b\c", 1),
        file(r"C:\a\b\c\d.txt", 2, 3),
        file(r"C:\a\b\e.txt", 1, 4),
        file(r"C:\a\f.txt", 0, 5),
    ]
}

#[test]
fn a_same_volume_move_renames_each_selected_item_once() {
    let plan = plan(&move_into(tree(), r"C:\x", true));
    assert_eq!(
        plan.steps,
        vec![
            Step::Rename {
                source: loc(r"C:\a\b"),
                target: loc(r"C:\x\b"),
            },
            Step::Rename {
                source: loc(r"C:\a\f.txt"),
                target: loc(r"C:\x\f.txt"),
            },
        ]
    );
}

#[test]
fn a_move_across_volumes_copies_then_removes_in_reverse_order() {
    let plan = plan(&move_into(tree(), r"D:\x", false));
    assert_eq!(
        plan.steps,
        vec![
            make_folder(r"D:\x\b"),
            make_folder(r"D:\x\b\c"),
            copy_file(r"C:\a\b\c\d.txt", r"D:\x\b\c\d.txt", 3),
            copy_file(r"C:\a\b\e.txt", r"D:\x\b\e.txt", 4),
            copy_file(r"C:\a\f.txt", r"D:\x\f.txt", 5),
            remove_file(r"C:\a\f.txt"),
            remove_file(r"C:\a\b\e.txt"),
            remove_file(r"C:\a\b\c\d.txt"),
            remove_folder(r"C:\a\b\c"),
            remove_folder(r"C:\a\b"),
        ]
    );
}

#[test]
fn a_move_across_volumes_totals_the_copy() {
    let plan = plan(&move_into(tree(), r"D:\x", false));
    assert_eq!(
        plan.totals,
        Totals {
            files: 3,
            folders: 2,
            bytes: 12
        }
    );
}

#[test]
fn a_link_moved_across_volumes_stays_at_the_source() {
    let plan = plan(&move_into(vec![link(r"C:\a\l", 0)], r"D:\x", false));
    assert_eq!(plan.steps, vec![]);
    assert!(plan.risks.contains(&Risk::LinksSkipped(1)));
}

#[test]
fn a_folder_holding_a_skipped_link_is_kept_at_the_source() {
    let items = vec![folder(r"C:\a\b", 0), link(r"C:\a\b\l", 1)];
    let plan = plan(&move_into(items, r"D:\x", false));
    assert_eq!(plan.steps, vec![make_folder(r"D:\x\b")]);
}

#[test]
fn a_skipped_conflict_keeps_its_source_and_its_folders() {
    let request = PlanRequest {
        existing: vec![existing(r"D:\x\b\e.txt", ItemKind::File, 1_000)],
        policy: ConflictPolicy::Skip,
        ..move_into(tree(), r"D:\x", false)
    };
    let plan = plan(&request);
    assert_eq!(
        plan.steps,
        vec![
            make_folder(r"D:\x\b"),
            make_folder(r"D:\x\b\c"),
            copy_file(r"C:\a\b\c\d.txt", r"D:\x\b\c\d.txt", 3),
            copy_file(r"C:\a\f.txt", r"D:\x\f.txt", 5),
            remove_file(r"C:\a\f.txt"),
            remove_file(r"C:\a\b\c\d.txt"),
            remove_folder(r"C:\a\b\c"),
        ]
    );
}

#[test]
fn a_same_volume_move_totals_what_it_moves() {
    let plan = plan(&move_into(tree(), r"C:\x", true));
    assert_eq!(
        plan.totals,
        Totals {
            files: 3,
            folders: 2,
            bytes: 12
        }
    );
}
