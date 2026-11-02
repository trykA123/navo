use super::*;

const LOWER: &str = r"C:\a\x.txt";
const UPPER: &str = r"C:\b\X.txt";

fn twins_request(policy: ConflictPolicy) -> PlanRequest {
    PlanRequest {
        policy,
        ..copy_into(vec![file(LOWER, 0, 5), file(UPPER, 0, 6)], r"D:\d")
    }
}

fn case_twins(policy: ConflictPolicy) -> OperationPlan {
    plan(&twins_request(policy))
}

fn second_conflict(resolution: Resolution) -> Vec<Conflict> {
    vec![Conflict {
        source: loc(UPPER),
        target: loc(r"D:\d\X.txt"),
        resolution,
    }]
}

#[test]
fn a_name_that_differs_only_by_case_skips_on_a_windows_target() {
    let plan = case_twins(ConflictPolicy::Skip);
    assert_eq!(
        (plan.steps, plan.conflicts),
        (
            vec![copy_file(LOWER, r"D:\d\x.txt", 5)],
            second_conflict(Resolution::Skip)
        )
    );
}

#[test]
fn overwrite_all_never_overwrites_another_item_of_the_same_job() {
    let plan = case_twins(ConflictPolicy::OverwriteAll);
    assert_eq!(
        (plan.steps, plan.conflicts),
        (
            vec![copy_file(LOWER, r"D:\d\x.txt", 5)],
            second_conflict(Resolution::Skip)
        )
    );
}

#[test]
fn rename_gives_a_case_twin_the_next_free_name() {
    let renamed = r"D:\d\X (2).txt";
    let plan = case_twins(ConflictPolicy::Rename);
    assert_eq!(
        (plan.steps, plan.conflicts),
        (
            vec![
                copy_file(LOWER, r"D:\d\x.txt", 5),
                copy_file(UPPER, renamed, 6),
            ],
            second_conflict(Resolution::Rename(loc(renamed)))
        )
    );
}

#[test]
fn ask_asks_about_a_case_twin() {
    let plan = case_twins(ConflictPolicy::Ask);
    assert_eq!(plan.conflicts, second_conflict(Resolution::Ask));
}

#[test]
fn names_that_differ_by_case_both_copy_to_a_case_sensitive_target() {
    let plan = plan(&copy_into(
        vec![file("/a/x.txt", 0, 5), file("/b/X.txt", 0, 6)],
        "/d",
    ));
    assert_eq!(
        (plan.steps, plan.conflicts),
        (
            vec![
                copy_file("/a/x.txt", "/d/x.txt", 5),
                copy_file("/b/X.txt", "/d/X.txt", 6),
            ],
            Vec::new()
        )
    );
}

#[test]
fn the_same_name_from_two_folders_is_planned_once() {
    let plan = plan(&PlanRequest {
        policy: ConflictPolicy::OverwriteAll,
        ..copy_into(vec![file("/a/x.txt", 0, 5), file("/b/x.txt", 0, 6)], "/d")
    });
    assert_eq!(plan.steps, vec![copy_file("/a/x.txt", "/d/x.txt", 5)]);
}

#[test]
fn an_existing_target_does_not_hide_a_case_twin_from_overwrite() {
    let plan = plan(&PlanRequest {
        existing: vec![existing(r"D:\d\x.txt", ItemKind::File, 1)],
        ..twins_request(ConflictPolicy::OverwriteAll)
    });
    assert_eq!(plan.steps, vec![copy_file(LOWER, r"D:\d\x.txt", 5)]);
}

#[test]
fn a_move_keeps_the_source_of_a_skipped_case_twin() {
    let plan = plan(&PlanRequest {
        policy: ConflictPolicy::OverwriteAll,
        ..request(
            OpKind::Move,
            vec![file(LOWER, 0, 5), file(UPPER, 0, 6)],
            Some(r"D:\d"),
        )
    });
    assert_eq!(
        plan.steps,
        vec![copy_file(LOWER, r"D:\d\x.txt", 5), remove_file(LOWER)]
    );
}

#[test]
fn a_name_whose_temporary_file_is_another_target_is_a_conflict() {
    let plan = plan(&PlanRequest {
        policy: ConflictPolicy::Skip,
        ..copy_into(vec![file("/a/x.navo-part", 0, 5), file("/a/x", 0, 6)], "/d")
    });
    assert_eq!(
        (plan.steps, plan.conflicts),
        (
            vec![copy_file("/a/x.navo-part", "/d/x.navo-part", 5)],
            vec![Conflict {
                source: loc("/a/x"),
                target: loc("/d/x"),
                resolution: Resolution::Skip,
            }]
        )
    );
}

#[test]
fn a_temporary_name_planned_after_its_file_is_no_conflict() {
    let plan = plan(&copy_into(
        vec![file("/a/x", 0, 6), file("/a/x.navo-part", 0, 5)],
        "/d",
    ));
    assert_eq!(plan.steps.len(), 2);
}

#[test]
fn a_same_volume_move_renames_only_one_of_two_case_twins() {
    let plan = plan(&PlanRequest {
        policy: ConflictPolicy::Skip,
        same_volume: true,
        ..request(
            OpKind::Move,
            vec![file(LOWER, 0, 5), file(r"C:\b\X.txt", 0, 6)],
            Some(r"C:\d"),
        )
    });
    assert_eq!(
        plan.steps,
        vec![Step::Rename {
            source: loc(LOWER),
            target: loc(r"C:\d\x.txt"),
        }]
    );
}
