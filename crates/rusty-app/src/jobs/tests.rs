use std::sync::{Arc, Mutex};

use rusty_core::{ConflictPolicy, OpKind, OperationPlan, PlanRequest, plan};
use rusty_fs::{CancelToken, FakeFs, JobContext, JobOutcome, Provider, run_job, scan};
use rusty_store::{Db, JobJournal, StepState};

use super::{SharedJournal, lock, log_factory, open, recover};

fn journal() -> SharedJournal {
    let db = Db::open_in_memory().unwrap();
    Arc::new(Mutex::new(JobJournal::new(db).unwrap()))
}

fn fake() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file("home/docs/report.txt", 7);
    fs.add_folder("dest");
    fs
}

fn copy_plan(fs: &FakeFs) -> OperationPlan {
    let items = scan(fs, &[fs.location("home/docs")], &CancelToken::new()).unwrap();
    plan(&PlanRequest {
        kind: OpKind::Copy,
        items,
        target: Some(fs.location("dest")),
        existing: Vec::new(),
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: Vec::new(),
    })
}

fn exists(fs: &FakeFs, path: &str) -> bool {
    fs.metadata(&fs.location(path)).is_ok()
}

fn run(fs: &FakeFs, plan: &OperationPlan, journal: &SharedJournal, finish: bool) {
    let mut log = log_factory(Arc::clone(journal))(plan).unwrap();
    let outcome = run_job(
        plan,
        &[],
        &mut JobContext {
            ops: fs,
            log: log.as_mut(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
    if finish {
        log.finish(&outcome);
    }
}

#[test]
fn every_step_of_a_job_is_journaled() {
    let fs = fake();
    let journal = journal();
    let plan = copy_plan(&fs);
    run(&fs, &plan, &journal, false);
    let jobs = lock(&journal).unfinished().unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].kind, "copy");
    assert_eq!(jobs[0].steps.len(), plan.steps.len());
    assert!(jobs[0].steps.iter().all(|s| s.state == StepState::Done));
}

#[test]
fn a_job_cut_short_is_rolled_back_at_start() {
    let fs = fake();
    let journal = journal();
    run(&fs, &copy_plan(&fs), &journal, false);
    assert!(exists(&fs, "dest/docs/report.txt"));
    assert_eq!(recover(&journal, &fs).unwrap(), 1);
    assert!(!exists(&fs, "dest/docs"));
    assert!(exists(&fs, "home/docs/report.txt"));
    assert!(lock(&journal).unfinished().unwrap().is_empty());
}

#[test]
fn a_finished_job_is_left_alone() {
    let fs = fake();
    let journal = journal();
    run(&fs, &copy_plan(&fs), &journal, true);
    assert_eq!(recover(&journal, &fs).unwrap(), 0);
    assert!(exists(&fs, "dest/docs/report.txt"));
}

#[test]
fn a_half_written_copy_is_removed() {
    let fs = fake();
    let journal = journal();
    let plan = copy_plan(&fs);
    let bytes = serde_json::to_vec(&plan).unwrap();
    let steps = u32::try_from(plan.steps.len()).unwrap();
    lock(&journal).create_job("copy", &bytes, steps, 0).unwrap();
    fs.add_file("dest/docs/report.txt.ferrite-part", 3);
    assert_eq!(recover(&journal, &fs).unwrap(), 1);
    assert!(!exists(&fs, "dest/docs/report.txt.ferrite-part"));
}

#[test]
fn an_unreadable_plan_is_closed_without_touching_files() {
    let fs = fake();
    let journal = journal();
    lock(&journal)
        .create_job("copy", b"not a plan", 0, 0)
        .unwrap();
    assert_eq!(recover(&journal, &fs).unwrap(), 1);
    assert!(lock(&journal).unfinished().unwrap().is_empty());
    assert!(exists(&fs, "home/docs/report.txt"));
}

#[test]
fn the_journal_file_keeps_jobs_across_opens() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.db");
    let fs = fake();
    run(&fs, &copy_plan(&fs), &open(&file).unwrap(), false);
    let reopened = open(&file).unwrap();
    assert_eq!(lock(&reopened).unfinished().unwrap().len(), 1);
}

#[test]
fn only_one_process_can_claim_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.lock");
    let first = super::claim(&file);
    assert!(first.is_some());
    assert!(super::claim(&file).is_none());
    drop(first);
    assert!(super::claim(&file).is_some());
}
