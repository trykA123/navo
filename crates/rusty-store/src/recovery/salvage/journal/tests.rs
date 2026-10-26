use super::*;
use crate::{Db, JobJournal};

fn plan() -> Vec<u8> {
    serde_json::to_vec(&OperationPlan {
        kind: rusty_core::OpKind::Copy,
        steps: vec![],
        conflicts: vec![],
        risks: vec![],
        totals: rusty_core::Totals {
            files: 0,
            folders: 0,
            bytes: 0,
        },
    })
    .unwrap()
}

#[test]
fn proof_bearing_undo_with_a_legacy_owner_prevents_journal_salvage() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ferrite.db");
    let (db, _) = Db::open(&path, false).unwrap();
    let mut journal = JobJournal::new(db).unwrap();
    let plan = plan();
    let job = journal.create_job("copy", &plan, 0, 100).unwrap();
    journal.record_held(job, br#"{"proof":null}"#, 100).unwrap();
    let conn = Connection::open(path).unwrap();
    assert!(validate(&conn).is_err());
}

#[test]
fn overwrite_retention_marker_requires_its_distinct_owner_kind() {
    let mut plan: serde_json::Value = serde_json::from_slice(&plan()).unwrap();
    plan["overwrite_retention_version"] = 1.into();
    assert!(validate_plan("copy", "running", &serde_json::to_vec(&plan).unwrap()).is_err());
}

#[test]
fn temporary_namespace_requires_a_retained_owner_kind() {
    let mut plan: serde_json::Value = serde_json::from_slice(&plan()).unwrap();
    plan["temporary_namespace"] = "abc-123".into();
    assert!(validate_plan("copy", "running", &serde_json::to_vec(&plan).unwrap()).is_err());
}
