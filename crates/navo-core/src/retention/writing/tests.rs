use super::*;
use crate::{Location, Totals};

fn plan(kind: OpKind) -> OperationPlan {
    OperationPlan {
        kind,
        steps: vec![Step::CopyFile {
            source: Location::new("/source/a").unwrap(),
            target: Location::new("/dest/a").unwrap(),
            size: 1,
        }],
        totals: Totals {
            files: 1,
            folders: 0,
            bytes: 1,
        },
        conflicts: vec![],
        risks: vec![],
    }
}

#[test]
fn copy_and_move_have_distinct_validated_retention_kinds_and_round_trip() {
    for kind in [OpKind::Copy, OpKind::Move] {
        let retained = RetainedWritePlan::new(plan(kind), RetentionPolicy::default()).unwrap();
        assert!(is_retained_write(retained.journal_kind()));
        let bytes = serde_json::to_vec(&retained).unwrap();
        assert_eq!(
            serde_json::from_slice::<RetainedWritePlan>(&bytes).unwrap(),
            retained
        );
        assert_eq!(
            serde_json::from_slice::<OperationPlan>(&bytes).unwrap(),
            plan(kind)
        );
    }
}

#[test]
fn delete_steps_and_unsupported_versions_do_not_become_retained_copy_jobs() {
    assert!(RetainedWritePlan::new(plan(OpKind::Delete), RetentionPolicy::default()).is_err());
    let mut copy = plan(OpKind::Copy);
    copy.steps = vec![Step::RemoveFile {
        location: Location::new("/source/a").unwrap(),
    }];
    assert!(RetainedWritePlan::new(copy, RetentionPolicy::default()).is_err());
    let mut value = serde_json::to_value(
        RetainedWritePlan::new(plan(OpKind::Copy), RetentionPolicy::default()).unwrap(),
    )
    .unwrap();
    value["overwrite_retention_version"] = 2.into();
    assert!(serde_json::from_value::<RetainedWritePlan>(value).is_err());
}

#[test]
fn namespaced_copy_plans_round_trip_without_changing_legacy_json() {
    let old = RetainedWritePlan::new(plan(OpKind::Copy), Default::default()).unwrap();
    let old_value = serde_json::to_value(&old).unwrap();
    assert_eq!(old_value["overwrite_retention_version"], 1);
    assert!(old_value.get("temporary_namespace").is_none());
    let owned = old.with_temporary_namespace("ab-12".into()).unwrap();
    let value = serde_json::to_value(&owned).unwrap();
    assert_eq!(value["overwrite_retention_version"], 2);
    assert_eq!(
        serde_json::from_value::<RetainedWritePlan>(value).unwrap(),
        owned
    );
    for namespace in ["", "../escape", "not-hex", &"a".repeat(81)] {
        assert!(
            RetainedWritePlan::new(plan(OpKind::Copy), Default::default())
                .unwrap()
                .with_temporary_namespace(namespace.into())
                .is_err()
        );
    }
}

#[test]
fn a_copy_temporary_is_bound_to_its_sequence_target_and_scope() {
    use crate::{FileIdentity, FileIdentityKind};
    let owner = RetainedWritePlan::new(plan(OpKind::Copy), Default::default())
        .unwrap()
        .with_temporary_namespace("ab".into())
        .unwrap();
    let target = Location::new("/dest/a").unwrap();
    let identity = FileIdentity {
        volume: 1,
        file: [1; 16],
        kind: FileIdentityKind::Native,
    };
    let proof = super::super::CopyTemporary {
        temporary_version: 1,
        seq: 0,
        target: target.clone(),
        temporary: super::super::copy_temporary(&target, 0, Some("ab")).unwrap(),
        parent_identity: identity,
        mount_identity: 1,
        identity: Some(identity),
    };
    proof.validate(&owner).unwrap();
    let mut wrong = proof.clone();
    wrong.seq = 1;
    assert!(wrong.validate(&owner).is_err());
    let mut wrong = proof.clone();
    wrong.temporary = target;
    assert!(wrong.validate(&owner).is_err());
    let mut wrong = proof;
    wrong.identity.as_mut().unwrap().volume = 2;
    assert!(wrong.validate(&owner).is_err());
}

#[test]
fn owned_temporary_names_stay_bounded_for_a_maximum_length_destination_name() {
    let target = Location::new(format!("/dest/{}", "a".repeat(255))).unwrap();
    let temporary = super::super::copy_temporary(&target, u32::MAX, Some(&"a".repeat(80))).unwrap();
    assert!(temporary.name().unwrap().to_text().len() < 255);
    assert_eq!(temporary.parent(), target.parent());
}
