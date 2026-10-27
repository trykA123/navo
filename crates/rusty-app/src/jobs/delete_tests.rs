use std::sync::{Arc, Mutex};

use rusty_core::{ConflictPolicy, OpKind, OperationPlan, PlanRequest, Step, plan};
use rusty_fs::{
    CancelToken, FakeFs, JobContext, JobOutcome, Provider, ProviderOps, StepOutcome,
    run_job_parallel, scan,
};
use rusty_store::{Db, JobJournal, JobState, StepState};

use super::{SharedJournal, lock, log_factory, recover};

const FILES: usize = 40;

fn journal() -> SharedJournal {
    Arc::new(Mutex::new(
        JobJournal::new(Db::open_in_memory().unwrap()).unwrap(),
    ))
}

fn tree() -> FakeFs {
    let fs = FakeFs::new();
    for index in 0..FILES {
        fs.add_file(&format!("home/old/d{}/f{index}.txt", index % 4), 1);
    }
    fs
}

fn delete_plan(fs: &FakeFs) -> OperationPlan {
    plan(&PlanRequest {
        kind: OpKind::Delete,
        items: scan(fs, &[fs.location("home/old")], &CancelToken::new()).unwrap(),
        target: None,
        existing: Vec::new(),
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: Vec::new(),
    })
}

fn files_of(plan: &OperationPlan) -> Vec<(u32, &rusty_core::Location)> {
    (0_u32..)
        .zip(&plan.steps)
        .filter_map(|(seq, step)| match step {
            Step::RemoveFile { location } => Some((seq, location)),
            _ => None,
        })
        .collect()
}

fn states(journal: &SharedJournal) -> Vec<StepState> {
    let job = lock(journal).unfinished().unwrap().remove(0);
    job.steps.iter().map(|step| step.state).collect()
}

#[test]
fn a_delete_journals_its_removals_in_one_batch() {
    let (fs, journal) = (tree(), journal());
    let plan = delete_plan(&fs);
    let mut log = log_factory(Arc::clone(&journal))(&plan).unwrap();
    let outcome = run_job_parallel(
        &plan,
        &[],
        &mut JobContext {
            ops: &fs,
            log: log.as_mut(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
        4,
    );
    assert_eq!(outcome, JobOutcome::Done);
    assert!(
        states(&journal)
            .iter()
            .all(|state| *state == StepState::Done)
    );
    assert!(fs.metadata(&fs.location("home/old")).is_err());
}

#[test]
fn a_delete_that_crashed_with_unjournalled_removals_recovers_without_touching_files() {
    let (fs, journal) = (tree(), journal());
    let plan = delete_plan(&fs);
    let files = files_of(&plan);
    let mut log = log_factory(Arc::clone(&journal))(&plan).unwrap();
    let marked: Vec<(u32, StepOutcome)> = files[..10]
        .iter()
        .map(|(seq, _)| (*seq, StepOutcome::Done))
        .collect();
    for (_, location) in &files[..25] {
        fs.remove_file(location).unwrap();
    }
    log.steps(&marked).unwrap();
    drop(log);
    let job = lock(&journal).unfinished().unwrap().remove(0).id;
    assert_eq!(recover(&journal, &fs).unwrap(), 1);
    let (state, _) = lock(&journal).read_job(job).unwrap();
    let remaining = files
        .iter()
        .filter(|(_, location)| fs.metadata(location).is_ok())
        .count();
    assert_eq!((state, remaining), (JobState::RolledBack, FILES - 25));
}
