use super::*;

fn nested_tree() -> Vec<ScannedItem> {
    vec![
        folder(r"C:\a\b", 0),
        folder(r"C:\a\b\c", 1),
        file(r"C:\a\b\c\d.txt", 2, 3),
        file(r"C:\a\b\e.txt", 1, 4),
    ]
}

#[test]
fn one_file_becomes_one_copy_step() {
    let plan = plan(&copy_into(vec![file(r"C:\a\x.txt", 0, 5)], r"D:\x"));
    assert_eq!(plan.steps, vec![copy_file(r"C:\a\x.txt", r"D:\x\x.txt", 5)]);
}

#[test]
fn the_plan_keeps_its_kind() {
    let plan = plan(&copy_into(vec![file(r"C:\a\x.txt", 0, 5)], r"D:\x"));
    assert_eq!(plan.kind, OpKind::Copy);
}

#[test]
fn a_plain_copy_has_no_risks() {
    let plan = plan(&copy_into(vec![file(r"C:\a\x.txt", 0, 5)], r"D:\x"));
    assert_eq!(plan.risks, Vec::new());
}

#[test]
fn a_folder_is_made_before_its_children_in_scan_order() {
    let plan = plan(&copy_into(nested_tree(), r"D:\x"));
    assert_eq!(
        plan.steps,
        vec![
            make_folder(r"D:\x\b"),
            make_folder(r"D:\x\b\c"),
            copy_file(r"C:\a\b\c\d.txt", r"D:\x\b\c\d.txt", 3),
            copy_file(r"C:\a\b\e.txt", r"D:\x\b\e.txt", 4),
        ]
    );
}

#[test]
fn targets_are_relative_to_the_parent_of_each_selected_item() {
    let items = vec![
        file(r"C:\one\a.txt", 0, 1),
        file(r"C:\two\deep\b.txt", 0, 2),
    ];
    let plan = plan(&copy_into(items, r"D:\x"));
    assert_eq!(
        plan.steps,
        vec![
            copy_file(r"C:\one\a.txt", r"D:\x\a.txt", 1),
            copy_file(r"C:\two\deep\b.txt", r"D:\x\b.txt", 2),
        ]
    );
}

#[test]
fn unix_paths_are_planned_the_same_way() {
    let items = vec![folder("/home/a/b", 0), file("/home/a/b/c.txt", 1, 7)];
    let plan = plan(&copy_into(items, "/mnt/x"));
    assert_eq!(
        plan.steps,
        vec![
            make_folder("/mnt/x/b"),
            copy_file("/home/a/b/c.txt", "/mnt/x/b/c.txt", 7),
        ]
    );
}

#[test]
fn a_link_is_left_out_of_the_steps() {
    let items = vec![file(r"C:\a\x.txt", 0, 5), link(r"C:\a\l", 0)];
    let plan = plan(&copy_into(items, r"D:\x"));
    assert_eq!(plan.steps, vec![copy_file(r"C:\a\x.txt", r"D:\x\x.txt", 5)]);
}

#[test]
fn skipped_links_are_counted_as_a_risk() {
    let items = vec![
        folder(r"C:\a\b", 0),
        link(r"C:\a\b\l1", 1),
        link(r"C:\a\l2", 0),
    ];
    let plan = plan(&copy_into(items, r"D:\x"));
    assert!(plan.risks.contains(&Risk::LinksSkipped(2)));
}

#[test]
fn totals_count_copied_files_made_folders_and_bytes() {
    let plan = plan(&copy_into(nested_tree(), r"D:\x"));
    assert_eq!(
        plan.totals,
        Totals {
            files: 2,
            folders: 2,
            bytes: 7
        }
    );
}

#[test]
fn a_copy_without_conflicts_lists_none() {
    let plan = plan(&copy_into(nested_tree(), r"D:\x"));
    assert_eq!(plan.conflicts, Vec::new());
}
