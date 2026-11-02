use super::*;

const BASE: &str = r"C:\w";

fn twins() -> Vec<ScannedItem> {
    vec![file(r"C:\w\src\can\a.c", 0, 3), file(r"C:\w\lib\a.c", 0, 4)]
}

fn keeping(request: &PlanRequest) -> OperationPlan {
    plan_keeping(request, &loc(BASE))
}

#[test]
fn keeping_structure_makes_each_relative_folder_once_before_its_files() {
    let mut items = twins();
    items.push(file(r"C:\w\src\can\b.c", 0, 5));
    let plan = keeping(&copy_into(items, r"D:\x"));
    assert_eq!(
        plan.steps,
        vec![
            make_folder(r"D:\x\src"),
            make_folder(r"D:\x\src\can"),
            copy_file(r"C:\w\src\can\a.c", r"D:\x\src\can\a.c", 3),
            make_folder(r"D:\x\lib"),
            copy_file(r"C:\w\lib\a.c", r"D:\x\lib\a.c", 4),
            copy_file(r"C:\w\src\can\b.c", r"D:\x\src\can\b.c", 5),
        ]
    );
    assert!(plan.conflicts.is_empty());
    assert_eq!(plan.totals.folders, 3);
}

#[test]
fn keeping_structure_reuses_a_folder_that_is_already_there() {
    let request = PlanRequest {
        existing: vec![existing(r"D:\x\src", ItemKind::Folder, 1)],
        ..copy_into(vec![file(r"C:\w\src\can\a.c", 0, 3)], r"D:\x")
    };
    assert_eq!(
        keeping(&request).steps,
        vec![
            make_folder(r"D:\x\src\can"),
            copy_file(r"C:\w\src\can\a.c", r"D:\x\src\can\a.c", 3),
        ]
    );
}

#[test]
fn a_file_where_a_kept_folder_belongs_skips_the_item() {
    let request = PlanRequest {
        existing: vec![existing(r"D:\x\src", ItemKind::File, 1)],
        ..copy_into(twins(), r"D:\x")
    };
    let plan = keeping(&request);
    assert_eq!(
        plan.steps,
        vec![
            make_folder(r"D:\x\lib"),
            copy_file(r"C:\w\lib\a.c", r"D:\x\lib\a.c", 4),
        ]
    );
    assert_eq!(
        plan.conflicts,
        vec![Conflict {
            source: loc(r"C:\w\src\can\a.c"),
            target: loc(r"D:\x\src"),
            resolution: Resolution::Skip,
        }]
    );
}

#[test]
fn an_existing_file_inside_a_kept_folder_asks() {
    let request = PlanRequest {
        existing: vec![
            existing(r"D:\x\lib", ItemKind::Folder, 1),
            existing(r"D:\x\lib\a.c", ItemKind::File, 1),
        ],
        ..copy_into(vec![file(r"C:\w\lib\a.c", 0, 4)], r"D:\x")
    };
    let plan = keeping(&request);
    assert!(plan.steps.is_empty());
    assert_eq!(plan.conflicts[0].resolution, Resolution::Ask);
}

#[test]
fn keeping_structure_copies_a_selected_folder_under_its_relative_folder() {
    let items = vec![folder(r"C:\w\src\can", 0), file(r"C:\w\src\can\a.c", 1, 3)];
    assert_eq!(
        keeping(&copy_into(items, r"D:\x")).steps,
        vec![
            make_folder(r"D:\x\src"),
            make_folder(r"D:\x\src\can"),
            copy_file(r"C:\w\src\can\a.c", r"D:\x\src\can\a.c", 3),
        ]
    );
}

#[test]
fn an_item_outside_the_base_lands_flat() {
    let items = vec![file(r"E:\other\a.c", 0, 3)];
    assert_eq!(
        keeping(&copy_into(items, r"D:\x")).steps,
        vec![copy_file(r"E:\other\a.c", r"D:\x\a.c", 3)]
    );
}

#[test]
fn flat_twins_from_two_folders_ask_once_and_never_overwrite() {
    let plan = plan(&copy_into(twins(), r"D:\x"));
    assert_eq!(
        plan.steps,
        vec![copy_file(r"C:\w\src\can\a.c", r"D:\x\a.c", 3)]
    );
    assert_eq!(
        plan.conflicts,
        vec![Conflict {
            source: loc(r"C:\w\lib\a.c"),
            target: loc(r"D:\x\a.c"),
            resolution: Resolution::Ask,
        }]
    );
}

#[test]
fn a_same_volume_move_keeping_structure_makes_folders_then_renames() {
    let request = PlanRequest {
        same_volume: true,
        ..request(OpKind::Move, twins(), Some(r"C:\x"))
    };
    let plan = keeping(&request);
    assert_eq!(
        plan.steps,
        vec![
            make_folder(r"C:\x\src"),
            make_folder(r"C:\x\src\can"),
            Step::Rename {
                source: loc(r"C:\w\src\can\a.c"),
                target: loc(r"C:\x\src\can\a.c"),
            },
            make_folder(r"C:\x\lib"),
            Step::Rename {
                source: loc(r"C:\w\lib\a.c"),
                target: loc(r"C:\x\lib\a.c"),
            },
        ]
    );
    assert_eq!(plan.totals.files, 2);
}

#[test]
fn a_cross_volume_move_keeping_structure_removes_only_the_moved_files() {
    let plan = keeping(&request(OpKind::Move, twins(), Some(r"D:\x")));
    let removed: Vec<_> = plan
        .steps
        .iter()
        .filter(|step| matches!(step, Step::RemoveFile { .. } | Step::RemoveFolder { .. }))
        .cloned()
        .collect();
    assert_eq!(
        removed,
        vec![
            remove_file(r"C:\w\lib\a.c"),
            remove_file(r"C:\w\src\can\a.c")
        ]
    );
}
