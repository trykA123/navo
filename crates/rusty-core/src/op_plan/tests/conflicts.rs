use super::*;

const SOURCE: &str = r"C:\a\x.txt";
const TARGET: &str = r"D:\d\x.txt";

fn source(modified_ms: i64) -> ScannedItem {
    ScannedItem {
        modified_ms: Some(modified_ms),
        ..file(SOURCE, 0, 5)
    }
}

fn clash(policy: ConflictPolicy, source_ms: i64, target_ms: i64) -> OperationPlan {
    plan(&PlanRequest {
        existing: vec![existing(TARGET, ItemKind::File, target_ms)],
        policy,
        ..copy_into(vec![source(source_ms)], r"D:\d")
    })
}

fn conflict(resolution: Resolution) -> Vec<Conflict> {
    vec![Conflict {
        source: loc(SOURCE),
        target: loc(TARGET),
        resolution,
    }]
}

fn outcome(plan: OperationPlan) -> (Vec<Step>, Vec<Conflict>) {
    (plan.steps, plan.conflicts)
}

#[test]
fn skip_leaves_the_file_out() {
    assert_eq!(
        outcome(clash(ConflictPolicy::Skip, 2_000, 1_000)),
        (Vec::new(), conflict(Resolution::Skip))
    );
}

#[test]
fn rename_copies_to_the_second_name() {
    let renamed = r"D:\d\x (2).txt";
    assert_eq!(
        outcome(clash(ConflictPolicy::Rename, 2_000, 1_000)),
        (
            vec![copy_file(SOURCE, renamed, 5)],
            conflict(Resolution::Rename(loc(renamed)))
        )
    );
}

#[test]
fn overwrite_all_overwrites_a_newer_target() {
    assert_eq!(
        outcome(clash(ConflictPolicy::OverwriteAll, 1_000, 2_000)),
        (
            vec![copy_file(SOURCE, TARGET, 5)],
            conflict(Resolution::Overwrite)
        )
    );
}

#[test]
fn overwrite_older_overwrites_when_the_source_is_newer() {
    assert_eq!(
        outcome(clash(ConflictPolicy::OverwriteOlder, 2_000, 1_000)),
        (
            vec![copy_file(SOURCE, TARGET, 5)],
            conflict(Resolution::Overwrite)
        )
    );
}

#[test]
fn overwrite_older_skips_when_the_source_is_older() {
    assert_eq!(
        outcome(clash(ConflictPolicy::OverwriteOlder, 1_000, 2_000)),
        (Vec::new(), conflict(Resolution::Skip))
    );
}

#[test]
fn overwrite_older_skips_when_both_have_the_same_time() {
    assert_eq!(
        outcome(clash(ConflictPolicy::OverwriteOlder, 1_000, 1_000)),
        (Vec::new(), conflict(Resolution::Skip))
    );
}

#[test]
fn ask_leaves_the_file_out_until_someone_answers() {
    assert_eq!(
        outcome(clash(ConflictPolicy::Ask, 2_000, 1_000)),
        (Vec::new(), conflict(Resolution::Ask))
    );
}

#[test]
fn a_moved_file_conflicts_the_same_way() {
    let plan = plan(&PlanRequest {
        kind: OpKind::Move,
        existing: vec![existing(TARGET, ItemKind::File, 1_000)],
        policy: ConflictPolicy::Skip,
        ..copy_into(vec![source(2_000)], r"D:\d")
    });
    assert_eq!(plan.conflicts, conflict(Resolution::Skip));
}

#[test]
fn rename_takes_the_third_name_when_the_second_exists() {
    let plan = plan(&PlanRequest {
        existing: vec![
            existing(TARGET, ItemKind::File, 1_000),
            existing(r"D:\d\x (2).txt", ItemKind::File, 1_000),
        ],
        policy: ConflictPolicy::Rename,
        ..copy_into(vec![source(2_000)], r"D:\d")
    });
    assert_eq!(plan.steps, vec![copy_file(SOURCE, r"D:\d\x (3).txt", 5)]);
}

#[test]
fn rename_skips_a_name_already_planned() {
    let plan = plan(&PlanRequest {
        existing: vec![existing(TARGET, ItemKind::File, 1_000)],
        policy: ConflictPolicy::Rename,
        ..copy_into(
            vec![file(r"C:\one\x.txt", 0, 1), file(r"C:\two\x.txt", 0, 2)],
            r"D:\d",
        )
    });
    assert_eq!(
        plan.steps,
        vec![
            copy_file(r"C:\one\x.txt", r"D:\d\x (2).txt", 1),
            copy_file(r"C:\two\x.txt", r"D:\d\x (3).txt", 2),
        ]
    );
}

#[test]
fn rename_appends_the_number_to_a_name_without_extension() {
    let plan = plan(&PlanRequest {
        existing: vec![existing(r"D:\d\notes", ItemKind::File, 1_000)],
        policy: ConflictPolicy::Rename,
        ..copy_into(vec![file(r"C:\a\notes", 0, 5)], r"D:\d")
    });
    assert_eq!(
        plan.steps,
        vec![copy_file(r"C:\a\notes", r"D:\d\notes (2)", 5)]
    );
}

fn folder_onto(kind: ItemKind) -> OperationPlan {
    plan(&PlanRequest {
        existing: vec![existing(r"D:\d\b", kind, 1_000)],
        policy: ConflictPolicy::OverwriteAll,
        ..copy_into(
            vec![folder(r"C:\a\b", 0), file(r"C:\a\b\c.txt", 1, 3)],
            r"D:\d",
        )
    })
}

#[test]
fn a_folder_onto_a_folder_merges_without_making_it() {
    assert_eq!(
        folder_onto(ItemKind::Folder).steps,
        vec![copy_file(r"C:\a\b\c.txt", r"D:\d\b\c.txt", 3)]
    );
}

#[test]
fn a_folder_merge_is_not_a_conflict() {
    assert_eq!(folder_onto(ItemKind::Folder).conflicts, Vec::new());
}

#[test]
fn a_folder_onto_a_file_skips_the_whole_subtree() {
    assert_eq!(folder_onto(ItemKind::File).steps, Vec::new());
}

#[test]
fn a_folder_onto_a_file_is_a_skipped_conflict_whatever_the_policy() {
    assert_eq!(
        folder_onto(ItemKind::File).conflicts,
        vec![Conflict {
            source: loc(r"C:\a\b"),
            target: loc(r"D:\d\b"),
            resolution: Resolution::Skip,
        }]
    );
}

fn same_place(policy: ConflictPolicy) -> Vec<Step> {
    plan(&PlanRequest {
        existing: vec![existing(SOURCE, ItemKind::File, 1_000)],
        policy,
        ..copy_into(vec![source(1_000)], r"C:\a")
    })
    .steps
}

#[test]
fn a_same_place_copy_with_rename_gets_the_second_name() {
    assert_eq!(
        same_place(ConflictPolicy::Rename),
        vec![copy_file(SOURCE, r"C:\a\x (2).txt", 5)]
    );
}

#[test]
fn a_same_place_copy_with_ask_gets_the_second_name() {
    assert_eq!(
        same_place(ConflictPolicy::Ask),
        vec![copy_file(SOURCE, r"C:\a\x (2).txt", 5)]
    );
}

#[test]
fn a_same_place_copy_with_skip_skips() {
    assert_eq!(same_place(ConflictPolicy::Skip), Vec::new());
}

#[test]
fn a_same_place_copy_never_overwrites_with_overwrite_all() {
    assert_eq!(same_place(ConflictPolicy::OverwriteAll), Vec::new());
}

#[test]
fn a_same_place_copy_never_overwrites_with_overwrite_older() {
    assert_eq!(same_place(ConflictPolicy::OverwriteOlder), Vec::new());
}

#[test]
fn same_place_follows_windows_case_rules() {
    let plan = plan(&PlanRequest {
        existing: vec![existing(SOURCE, ItemKind::File, 1_000)],
        policy: ConflictPolicy::OverwriteAll,
        ..copy_into(vec![source(1_000)], r"c:\A")
    });
    assert_eq!(plan.steps, Vec::new());
}
