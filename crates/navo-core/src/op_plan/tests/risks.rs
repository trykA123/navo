use super::*;

const GIB: u64 = 1_073_741_824;

fn files(count: u32) -> Vec<ScannedItem> {
    (0..count)
        .map(|index| file(&format!(r"C:\a\f{index}.txt"), 0, 1))
        .collect()
}

fn protected(text: &str) -> bool {
    is_protected(&loc(text), &[])
}

#[test]
fn copying_101_items_is_many() {
    let plan = plan(&copy_into(files(101), r"D:\d"));
    assert!(plan.risks.contains(&Risk::ManyItems(101)));
}

#[test]
fn copying_100_items_is_not_many() {
    let plan = plan(&copy_into(files(100), r"D:\d"));
    assert_eq!(plan.risks, Vec::new());
}

#[test]
fn deleting_101_items_is_many() {
    let plan = plan(&request(OpKind::Delete, files(101), None));
    assert!(plan.risks.contains(&Risk::ManyItems(101)));
}

#[test]
fn more_than_a_gib_is_large() {
    let items = vec![file(r"C:\a\x.bin", 0, GIB), file(r"C:\a\y.bin", 0, 1)];
    let plan = plan(&copy_into(items, r"D:\d"));
    assert!(plan.risks.contains(&Risk::Large(GIB + 1)));
}

#[test]
fn exactly_a_gib_is_not_large() {
    let plan = plan(&copy_into(vec![file(r"C:\a\x.bin", 0, GIB)], r"D:\d"));
    assert_eq!(plan.risks, Vec::new());
}

#[test]
fn recycling_more_than_a_gib_is_large() {
    let items = vec![folder(r"C:\a\b", 0), file(r"C:\a\b\x.bin", 1, GIB + 1)];
    let plan = plan(&request(OpKind::Recycle, items, None));
    assert!(plan.risks.contains(&Risk::Large(GIB + 1)));
}

#[test]
fn a_drive_root_is_protected() {
    assert!(protected(r"C:\"));
}

#[test]
fn another_drive_root_is_protected() {
    assert!(protected(r"D:\"));
}

#[test]
fn a_unc_share_root_is_protected() {
    assert!(protected(r"\\srv\share"));
}

#[test]
fn a_folder_in_a_unc_share_is_not_protected() {
    assert!(!protected(r"\\srv\share\docs"));
}

#[test]
fn the_windows_folder_is_protected() {
    assert!(protected(r"C:\Windows"));
}

#[test]
fn the_windows_folder_is_protected_in_any_case() {
    assert!(protected(r"c:\WINDOWS"));
}

#[test]
fn a_file_inside_the_windows_folder_is_not_protected_alone() {
    assert!(!protected(r"C:\Windows\System32\x.dll"));
}

#[test]
fn program_files_is_protected() {
    assert!(protected(r"C:\Program Files"));
}

#[test]
fn program_files_x86_is_protected() {
    assert!(protected(r"C:\Program Files (x86)"));
}

#[test]
fn program_files_is_protected_on_any_drive() {
    assert!(protected(r"E:\Program Files"));
}

#[test]
fn program_files_is_protected_in_any_case() {
    assert!(protected(r"d:\PROGRAM FILES (X86)"));
}

#[test]
fn an_app_inside_program_files_is_not_protected() {
    assert!(!protected(r"C:\Program Files\App"));
}

#[test]
fn the_unix_root_is_protected() {
    assert!(protected("/"));
}

#[test]
fn a_unix_folder_is_not_protected() {
    assert!(!protected("/home"));
}

#[test]
fn an_ordinary_folder_is_not_protected() {
    assert!(!protected(r"C:\Users\admin\docs"));
}

#[test]
fn an_extra_folder_is_protected() {
    assert!(is_protected(
        &loc(r"C:\Users\admin"),
        &[loc(r"C:\Users\admin")]
    ));
}

#[test]
fn an_extra_folder_is_matched_with_windows_case_rules() {
    assert!(is_protected(
        &loc(r"c:\users\ADMIN"),
        &[loc(r"C:\Users\admin")]
    ));
}

#[test]
fn a_child_of_an_extra_folder_is_not_protected() {
    assert!(!is_protected(
        &loc(r"C:\Users\admin\docs"),
        &[loc(r"C:\Users\admin")]
    ));
}

#[test]
fn deleting_a_protected_source_is_flagged() {
    let plan = plan(&request(
        OpKind::Delete,
        vec![folder(r"C:\Windows", 0)],
        None,
    ));
    assert!(plan.risks.contains(&Risk::Protected(loc(r"C:\Windows"))));
}

#[test]
fn copying_into_a_protected_target_is_flagged() {
    let plan = plan(&copy_into(
        vec![file(r"C:\a\x.txt", 0, 1)],
        r"C:\Program Files",
    ));
    assert!(
        plan.risks
            .contains(&Risk::Protected(loc(r"C:\Program Files")))
    );
}

#[test]
fn recycling_an_extra_protected_folder_is_flagged() {
    let plan = plan(&PlanRequest {
        protected: vec![loc(r"C:\Users\admin")],
        ..request(OpKind::Recycle, vec![folder(r"C:\Users\admin", 0)], None)
    });
    assert!(
        plan.risks
            .contains(&Risk::Protected(loc(r"C:\Users\admin")))
    );
}

#[test]
fn a_protected_folder_inside_a_deleted_subtree_is_flagged() {
    let plan = plan(&PlanRequest {
        protected: vec![loc(r"C:\Users\admin")],
        ..request(
            OpKind::Delete,
            vec![folder(r"C:\Users", 0), folder(r"C:\Users\admin", 1)],
            None,
        )
    });
    assert!(
        plan.risks
            .contains(&Risk::Protected(loc(r"C:\Users\admin")))
    );
}

fn into_itself(kind: OpKind, target: &str) -> OperationPlan {
    plan(&request(
        kind,
        vec![folder(r"C:\a\b", 0), file(r"C:\a\b\c.txt", 1, 3)],
        Some(target),
    ))
}

#[test]
fn copying_a_folder_into_itself_has_no_steps() {
    assert_eq!(into_itself(OpKind::Copy, r"C:\a\b").steps, Vec::new());
}

#[test]
fn copying_a_folder_into_its_subtree_has_no_steps() {
    assert_eq!(into_itself(OpKind::Copy, r"C:\a\b\c\d").steps, Vec::new());
}

#[test]
fn moving_a_folder_into_its_subtree_has_no_steps() {
    assert_eq!(into_itself(OpKind::Move, r"C:\a\b\c").steps, Vec::new());
}

#[test]
fn moving_a_folder_into_its_subtree_is_flagged() {
    assert!(
        into_itself(OpKind::Move, r"C:\a\b\c")
            .risks
            .contains(&Risk::Protected(loc(r"C:\a\b")))
    );
}

#[test]
fn into_itself_follows_windows_case_rules() {
    assert_eq!(into_itself(OpKind::Copy, r"c:\A\B\c").steps, Vec::new());
}

#[test]
fn into_itself_skips_only_that_item() {
    let plan = plan(&copy_into(
        vec![
            folder(r"C:\a\b", 0),
            file(r"C:\a\b\c.txt", 1, 3),
            file(r"C:\a\x.txt", 0, 5),
        ],
        r"C:\a\b",
    ));
    assert_eq!(
        plan.steps,
        vec![copy_file(r"C:\a\x.txt", r"C:\a\b\x.txt", 5)]
    );
}

fn plan_with(risks: Vec<Risk>) -> OperationPlan {
    OperationPlan {
        kind: OpKind::Copy,
        steps: Vec::new(),
        totals: Totals {
            files: 0,
            folders: 0,
            bytes: 0,
        },
        conflicts: Vec::new(),
        risks,
    }
}

#[test]
fn a_plan_without_risks_needs_no_confirmation() {
    assert!(!plan_with(Vec::new()).needs_confirmation());
}

#[test]
fn any_risk_needs_confirmation() {
    assert!(plan_with(vec![Risk::LinksSkipped(1)]).needs_confirmation());
}

#[test]
fn a_permanent_plan_needs_confirmation() {
    assert!(plan_with(vec![Risk::Permanent]).needs_confirmation());
}
