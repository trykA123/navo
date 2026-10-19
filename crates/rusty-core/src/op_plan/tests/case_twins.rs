use proptest::prelude::*;

use super::*;

const STEMS: [&str; 6] = ["x", "X", "x (2)", "X (2)", "x (3)", "X (3)"];

fn twins_plan(policy: ConflictPolicy, existing_items: Vec<ExistingItem>) -> OperationPlan {
    plan(&PlanRequest {
        policy,
        existing: existing_items,
        ..copy_into(vec![file("/s/A.txt", 0, 5), file("/s/a.txt", 0, 6)], "/d")
    })
}

fn conflict(source: &str, target: &str, resolution: Resolution) -> Conflict {
    Conflict {
        source: loc(source),
        target: loc(target),
        resolution,
    }
}

#[test]
fn case_twins_on_a_case_sensitive_target_are_two_conflicts() {
    let plan = twins_plan(
        ConflictPolicy::Ask,
        vec![
            existing("/d/A.txt", ItemKind::File, 1),
            existing("/d/a.txt", ItemKind::File, 1),
        ],
    );
    assert_eq!(
        plan.conflicts,
        vec![
            conflict("/s/A.txt", "/d/A.txt", Resolution::Ask),
            conflict("/s/a.txt", "/d/a.txt", Resolution::Ask),
        ]
    );
}

#[test]
fn each_case_twin_is_decided_against_its_own_existing_file() {
    let plan = twins_plan(
        ConflictPolicy::OverwriteOlder,
        vec![
            existing("/d/A.txt", ItemKind::File, 1),
            existing("/d/a.txt", ItemKind::File, 5_000),
        ],
    );
    assert_eq!(
        (plan.steps, plan.conflicts),
        (
            vec![copy_file("/s/A.txt", "/d/A.txt", 5)],
            vec![
                conflict("/s/A.txt", "/d/A.txt", Resolution::Overwrite),
                conflict("/s/a.txt", "/d/a.txt", Resolution::Skip),
            ]
        )
    );
}

#[test]
fn a_name_in_other_case_is_one_conflict_on_a_windows_target() {
    let plan = plan(&PlanRequest {
        existing: vec![existing(r"D:\d\A.txt", ItemKind::File, 1)],
        ..copy_into(vec![file(r"C:\s\a.txt", 0, 5)], r"D:\d")
    });
    assert_eq!(
        plan.conflicts,
        vec![conflict(r"C:\s\a.txt", r"D:\d\a.txt", Resolution::Ask)]
    );
}

#[test]
fn rename_skips_a_numbered_name_taken_in_another_case_twin() {
    let plan = plan(&PlanRequest {
        policy: ConflictPolicy::Rename,
        existing: vec![
            existing("/d/x.txt", ItemKind::File, 1),
            existing("/d/x (2).txt", ItemKind::File, 1),
            existing("/d/X (2).txt", ItemKind::File, 1),
        ],
        ..copy_into(vec![file("/s/x.txt", 0, 5)], "/d")
    });
    assert_eq!(plan.steps, vec![copy_file("/s/x.txt", "/d/x (3).txt", 5)]);
}

fn names(picks: &[usize]) -> Vec<String> {
    let mut names: Vec<String> = picks
        .iter()
        .map(|&i| format!("{}.txt", STEMS[i % STEMS.len()]))
        .collect();
    names.sort();
    names.dedup();
    names
}

fn random_request() -> impl Strategy<Value = PlanRequest> {
    (
        prop::collection::vec(0..STEMS.len(), 0..=6),
        prop::collection::vec(0..2_usize, 1..=2),
        any::<bool>(),
        prop_oneof![Just(ConflictPolicy::Rename), Just(ConflictPolicy::Ask)],
    )
        .prop_map(|(taken, sources, windows, policy)| {
            let (source, target) = if windows {
                (r"C:\s", r"D:\d")
            } else {
                ("/s", "/d")
            };
            let at = |folder: &str, name: &String| {
                loc(folder)
                    .join(&crate::ProviderPath::from_text(name))
                    .unwrap()
            };
            let items = names(&sources)
                .iter()
                .map(|name| ScannedItem {
                    location: at(source, name),
                    ..file(source, 0, 1)
                })
                .collect();
            let existing = names(&taken)
                .iter()
                .map(|name| ExistingItem {
                    location: at(target, name),
                    ..existing(target, ItemKind::File, 1)
                })
                .collect();
            PlanRequest {
                policy,
                existing,
                ..copy_into(items, target)
            }
        })
}

fn copy_targets(plan: &OperationPlan) -> Vec<&Location> {
    plan.steps
        .iter()
        .filter_map(|s| match s {
            Step::CopyFile { target, .. } => Some(target),
            _ => None,
        })
        .collect()
}

proptest! {
    #[test]
    fn a_copy_never_lands_on_an_existing_or_planned_name(request in random_request()) {
        let plan = plan(&request);
        let targets = copy_targets(&plan);
        for (index, target) in targets.iter().enumerate() {
            let taken = request.existing.iter().any(|e| e.location.same_place(target));
            prop_assert!(!taken, "{target:?} is taken");
            let twice = targets[..index].iter().any(|t| t.same_place(target));
            prop_assert!(!twice, "{target:?} is planned twice");
        }
    }

    #[test]
    fn every_existing_target_is_a_conflict(request in random_request()) {
        let plan = plan(&request);
        let target = request.target.clone().unwrap();
        for item in &request.items {
            let name = item.location.as_str().rsplit(['\\', '/']).next().unwrap();
            let destination = target.join(&crate::ProviderPath::from_text(name)).unwrap();
            if request.existing.iter().any(|e| e.location.same_place(&destination)) {
                let asked = plan.conflicts.iter().any(|c| c.source == item.location);
                prop_assert!(asked, "{destination:?} exists but is no conflict");
            }
        }
    }
}
