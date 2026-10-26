use super::*;
use crate::Totals;

fn plan() -> OperationPlan {
    OperationPlan {
        kind: OpKind::Delete,
        steps: vec![
            Step::RemoveFile {
                location: Location::new("/home/docs/a").unwrap(),
            },
            Step::RemoveFolder {
                location: Location::new("/home/docs").unwrap(),
            },
            Step::RemoveLink {
                location: Location::new("/home/link").unwrap(),
            },
        ],
        totals: Totals {
            files: 2,
            folders: 1,
            bytes: 4,
        },
        conflicts: vec![],
        risks: vec![],
    }
}

fn retained() -> RetainedDeletePlan {
    RetainedDeletePlan::new(plan(), RetentionPolicy::new(3, 1024, 0).unwrap()).unwrap()
}

#[test]
fn selected_parent_and_separate_link_are_retained_once() {
    assert_eq!(
        retained().roots(),
        vec![
            Location::new("/home/docs").unwrap(),
            Location::new("/home/link").unwrap()
        ]
    );
}

#[test]
fn persisted_policy_round_trips_and_preserves_manual_plan_readability() {
    let original = retained();
    let bytes = serde_json::to_vec(&original).unwrap();
    assert_eq!(
        serde_json::from_slice::<RetainedDeletePlan>(&bytes).unwrap(),
        original
    );
    assert_eq!(
        serde_json::from_slice::<OperationPlan>(&bytes).unwrap(),
        plan()
    );
}

#[test]
fn invalid_policy_and_missing_or_unknown_version_are_rejected() {
    let mut value = serde_json::to_value(retained()).unwrap();
    value["retention"]["days"] = 8.into();
    assert!(serde_json::from_value::<RetainedDeletePlan>(value).is_err());
    let mut value = serde_json::to_value(retained()).unwrap();
    value["retention_version"] = 2.into();
    assert!(serde_json::from_value::<RetainedDeletePlan>(value).is_err());
    assert!(
        serde_json::from_value::<RetainedDeletePlan>(serde_json::to_value(plan()).unwrap())
            .is_err()
    );
}

#[test]
fn mixed_and_duplicate_steps_are_rejected() {
    let mut mixed = plan();
    mixed.steps.push(Step::MakeFolder {
        target: Location::new("/dest").unwrap(),
    });
    assert!(RetainedDeletePlan::new(mixed, retained().policy()).is_err());
    let mut duplicate = plan();
    duplicate.steps.push(duplicate.steps[0].clone());
    assert!(RetainedDeletePlan::new(duplicate, retained().policy()).is_err());
}

#[test]
fn exact_node_limit_is_retained_and_only_a_valid_larger_delete_reports_its_limit() {
    let mut large = plan();
    large.steps = (0..MAX_NODES)
        .map(|index| Step::RemoveFile {
            location: Location::new(format!("/home/delete/{index}")).unwrap(),
        })
        .collect();
    assert!(!delete_exceeds_limit(&large));
    assert!(RetainedDeletePlan::new(large.clone(), RetentionPolicy::default()).is_ok());
    large.steps.push(Step::RemoveFolder {
        location: Location::new("/home/delete").unwrap(),
    });
    assert!(delete_exceeds_limit(&large));
    assert_eq!(
        RetainedDeletePlan::new(large.clone(), RetentionPolicy::default()).unwrap_err(),
        RetentionError::DeleteTooLarge
    );
    large.steps[1] = large.steps[0].clone();
    assert!(matches!(
        RetainedDeletePlan::new(large.clone(), RetentionPolicy::default()),
        Err(RetentionError::Invalid(_))
    ));
    large.kind = OpKind::Copy;
    assert!(!delete_exceeds_limit(&large));
    assert!(matches!(
        RetainedDeletePlan::new(large, RetentionPolicy::default()),
        Err(RetentionError::Invalid(_))
    ));
}

#[test]
fn oversized_delete_rejects_dot_components_before_offering_permanent_fallback() {
    let mut large = plan();
    large.steps = (0..=MAX_NODES)
        .map(|index| Step::RemoveFile {
            location: Location::new(format!("/home/delete/{index}")).unwrap(),
        })
        .collect();
    for path in ["/home/delete/./item", "/home/delete/../victim"] {
        large.steps[0] = Step::RemoveFile {
            location: Location::new(path).unwrap(),
        };
        assert!(matches!(
            RetainedDeletePlan::new(large.clone(), RetentionPolicy::default()),
            Err(RetentionError::Invalid(_))
        ));
    }
}

#[test]
fn capacity_plan_bytes_over_the_policy_cap_require_permanent_confirmation() {
    let mut oversized = plan();
    oversized.totals.bytes = 1025;
    assert!(matches!(
        RetainedDeletePlan::new(oversized, RetentionPolicy::new(3, 1024, 0).unwrap()),
        Err(RetentionError::DeleteTooLarge)
    ));
}
