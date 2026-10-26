use super::*;
use rusty_core::retention::MAX_NODES;
use rusty_core::{Step, Totals};

fn deletion(size: usize) -> OperationPlan {
    let fs = FakeFs::new();
    OperationPlan {
        kind: OpKind::Delete,
        steps: (0..size)
            .map(|index| Step::RemoveFile {
                location: fs.location(&format!("delete/{index}")),
            })
            .collect(),
        totals: Totals {
            files: size as u64,
            folders: 0,
            bytes: 0,
        },
        conflicts: vec![],
        risks: vec![],
    }
}

#[test]
fn an_approved_oversized_delete_uses_the_permanent_backend() {
    let (_root, dirs, journal) = setup();
    std::fs::write(dirs.settings_file(), "[retention]\nretain-deletes = true\n").unwrap();
    let plan = deletion(MAX_NODES + 1);
    let log = experimental_factory(journal.clone(), dirs)(&plan).unwrap();
    let job = super::super::super::lock(&journal)
        .unfinished()
        .unwrap()
        .remove(0);
    assert_eq!(job.kind, "delete");
    assert!(
        serde_json::from_slice::<rusty_core::retention::RetainedDeletePlan>(&job.plan).is_err()
    );
    assert_eq!(job.steps.len(), MAX_NODES + 1);
    drop(log);
}

#[test]
fn invalid_large_deletes_and_invalid_retention_settings_never_fall_back() {
    let (_root, dirs, journal) = setup();
    std::fs::write(dirs.settings_file(), "[retention]\nretain-deletes = true\n").unwrap();
    let factory = experimental_factory(journal, dirs.clone());
    let mut plan = deletion(MAX_NODES + 1);
    plan.steps[1] = plan.steps[0].clone();
    assert!(factory(&plan).is_err());
    plan.steps[1] = Step::MakeFolder {
        target: FakeFs::new().location("new"),
    };
    assert!(factory(&plan).is_err());
    std::fs::write(
        dirs.settings_file(),
        "[retention]\nretain-deletes = true\ndays = 8\n",
    )
    .unwrap();
    assert!(factory(&deletion(MAX_NODES + 1)).is_err());
}
