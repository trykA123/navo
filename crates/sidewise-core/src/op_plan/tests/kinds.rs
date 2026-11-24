//! Overwrite choices meet a name of another kind, or a whole folder: the plan skips it and adds
//! no step, so the summary never counts an overwrite the job engine would refuse mid-job.

use super::*;

/// Plans `items` into `D:\d` with `policy` over one existing `D:\d\b` of `kind`; `moving`
/// picks a same-drive move (top-level renames) instead of a copy.
fn onto(
    items: Vec<ScannedItem>,
    kind: ItemKind,
    policy: ConflictPolicy,
    moving: bool,
) -> OperationPlan {
    let kind_of_op = if moving { OpKind::Move } else { OpKind::Copy };
    plan(&PlanRequest {
        existing: vec![existing(r"D:\d\b", kind, 1_000)],
        policy,
        same_volume: moving,
        ..request(kind_of_op, items, Some(r"D:\d"))
    })
}

fn skipped(source: &str) -> Vec<Conflict> {
    vec![Conflict {
        source: loc(source),
        target: loc(r"D:\d\b"),
        resolution: Resolution::Skip,
    }]
}

fn newer_folder() -> Vec<ScannedItem> {
    let mut top = folder(r"C:\a\b", 0);
    top.modified_ms = Some(9_000);
    vec![top, file(r"C:\a\b\c.txt", 1, 3)]
}

#[test]
fn overwrite_all_skips_a_copied_file_onto_a_folder() {
    let plan = onto(
        vec![file(r"C:\a\b", 0, 5)],
        ItemKind::Folder,
        ConflictPolicy::OverwriteAll,
        false,
    );
    assert_eq!(
        (plan.steps, plan.conflicts),
        (Vec::new(), skipped(r"C:\a\b"))
    );
}

#[test]
fn overwrite_all_skips_a_moved_file_onto_a_folder() {
    let plan = onto(
        vec![file(r"C:\a\b", 0, 5)],
        ItemKind::Folder,
        ConflictPolicy::OverwriteAll,
        true,
    );
    assert_eq!(
        (plan.steps, plan.conflicts),
        (Vec::new(), skipped(r"C:\a\b"))
    );
}

#[test]
fn overwrite_all_skips_a_moved_folder_onto_a_file() {
    let plan = onto(
        newer_folder(),
        ItemKind::File,
        ConflictPolicy::OverwriteAll,
        true,
    );
    assert_eq!(
        (plan.steps, plan.conflicts),
        (Vec::new(), skipped(r"C:\a\b"))
    );
}

#[test]
fn overwrite_all_skips_a_moved_folder_onto_a_folder() {
    let plan = onto(
        newer_folder(),
        ItemKind::Folder,
        ConflictPolicy::OverwriteAll,
        true,
    );
    assert_eq!(
        (plan.steps, plan.conflicts),
        (Vec::new(), skipped(r"C:\a\b"))
    );
}

#[test]
fn overwrite_older_skips_a_newer_moved_folder_onto_a_folder() {
    let plan = onto(
        newer_folder(),
        ItemKind::Folder,
        ConflictPolicy::OverwriteOlder,
        true,
    );
    assert_eq!(
        (plan.steps, plan.conflicts),
        (Vec::new(), skipped(r"C:\a\b"))
    );
}

#[test]
fn overwrite_all_still_merges_a_copied_folder_onto_a_folder() {
    let plan = onto(
        newer_folder(),
        ItemKind::Folder,
        ConflictPolicy::OverwriteAll,
        false,
    );
    assert_eq!(
        (plan.steps, plan.conflicts),
        (
            vec![copy_file(r"C:\a\b\c.txt", r"D:\d\b\c.txt", 3)],
            Vec::new()
        )
    );
}

#[test]
fn overwrite_all_still_overwrites_a_moved_file_onto_a_file() {
    let plan = onto(
        vec![file(r"C:\a\b", 0, 5)],
        ItemKind::File,
        ConflictPolicy::OverwriteAll,
        true,
    );
    assert_eq!(
        plan.steps,
        vec![Step::Rename {
            source: loc(r"C:\a\b"),
            target: loc(r"D:\d\b"),
        }]
    );
    assert_eq!(plan.conflicts[0].resolution, Resolution::Overwrite);
}
