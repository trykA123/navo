use super::*;
use crate::{Db, JobJournal, JobState, RetentionCatalog};
use rusty_core::retention::{RetainedWritePlan, RetentionPolicy};
use rusty_core::{Location, OpKind, OperationPlan, Step, Totals};

fn owner(path: &std::path::Path) -> (JobJournal, JobId) {
    let plan = RetainedWritePlan::new(
        OperationPlan {
            kind: OpKind::Copy,
            steps: vec![Step::CopyFile {
                source: Location::new("/source/a").unwrap(),
                target: Location::new("/dest/a").unwrap(),
                size: 7,
            }],
            totals: Totals {
                files: 1,
                folders: 0,
                bytes: 7,
            },
            conflicts: vec![],
            risks: vec![],
        },
        RetentionPolicy::default(),
    )
    .unwrap();
    let (db, _) = Db::open(path, false).unwrap();
    let mut journal = JobJournal::new(db).unwrap();
    let job = journal
        .create_job(
            plan.journal_kind(),
            &serde_json::to_vec(&plan).unwrap(),
            1,
            100,
        )
        .unwrap();
    (journal, job)
}

fn refuses_completion(path: &std::path::Path, job: JobId) {
    let (db, _) = Db::open(path, false).unwrap();
    let mut catalog = RetentionCatalog::new(db).unwrap();
    assert!(
        catalog
            .transaction()
            .unwrap()
            .complete_overwrites(&RetainedCompletion {
                job,
                state: JobState::Done,
                records: &[],
                returned: &[],
            })
            .is_err()
    );
}

#[test]
fn overwrite_completion_refuses_unfinished_steps() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ferrite.db");
    let (journal, job) = owner(&path);
    refuses_completion(&path, job);
    assert_eq!(journal.read_job(job).unwrap().0, JobState::Running);
}

#[test]
fn overwrite_completion_refuses_shifted_step_sequences() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ferrite.db");
    let (journal, job) = owner(&path);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "UPDATE job_step SET seq = 1, state = 'done' WHERE job_id = ?1",
            [job.0],
        )
        .unwrap();
    refuses_completion(&path, job);
    assert_eq!(journal.read_job(job).unwrap().0, JobState::Running);
}

#[test]
fn completion_preserves_pending_temporaries_in_running_and_terminal_owners() {
    for state in [JobState::Running, JobState::Done] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("ferrite.db");
        let (mut journal, job) = owner(&path);
        journal
            .mark_step(job, 0, crate::StepState::Done, Some(b"pending temporary"))
            .unwrap();
        journal.finish(job, state).unwrap();
        refuses_completion(&path, job);
        assert_eq!(journal.read_job(job).unwrap().0, state);
        assert_eq!(
            journal.step_detail(job, 0).unwrap(),
            Some(b"pending temporary".to_vec())
        );
    }
}
