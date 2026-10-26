use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

use rusty_core::{ConflictPolicy, OpKind, OperationPlan, PlanRequest, ProviderPath, brand, plan};
use rusty_fs::{
    Attributes, CancelToken, EntryKind, FakeFs, FsEntry, JobContext, JobOutcome, Progress,
    Provider, run_job, scan,
};
use rusty_store::{Db, JobJournal, StepState};

use super::{SharedJournal, lock, log_factory, open, recover};

fn journal() -> SharedJournal {
    let db = Db::open_in_memory().unwrap();
    Arc::new(Mutex::new(JobJournal::new(db).unwrap()))
}

fn fake() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_entry(
        "home/docs",
        FsEntry {
            name: ProviderPath::from_text("report.txt"),
            kind: EntryKind::File,
            size: 7,
            modified_ms: Some(9_000),
            attributes: Attributes::default(),
        },
    );
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
        log.finish(&outcome, fs);
    }
}

#[test]
fn a_link_plan_round_trips_through_the_journal() {
    let fs = fake();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.db");
    let plan = plan(&PlanRequest {
        kind: OpKind::Link,
        items: scan(&fs, &[fs.location("home/docs")], &CancelToken::new()).unwrap(),
        target: Some(fs.location("dest")),
        existing: Vec::new(),
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: Vec::new(),
    });
    let journal = open(&file).unwrap();
    let log = log_factory(Arc::clone(&journal))(&plan).unwrap();
    drop((log, journal));
    let jobs = lock(&open(&file).unwrap()).unfinished().unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].kind, "link");
    assert_eq!(
        serde_json::from_slice::<OperationPlan>(&jobs[0].plan).unwrap(),
        plan
    );
    assert_eq!(jobs[0].steps.len(), 1);
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
    for suffix in [brand::TEMP_SUFFIX, brand::LEGACY_TEMP_SUFFIX] {
        let fs = fake();
        let journal = journal();
        let plan = copy_plan(&fs);
        let bytes = serde_json::to_vec(&plan).unwrap();
        let steps = u32::try_from(plan.steps.len()).unwrap();
        lock(&journal).create_job("copy", &bytes, steps, 0).unwrap();
        let part = format!("dest/docs/report.txt{suffix}");
        fs.add_file(&part, 3);
        assert_eq!(recover(&journal, &fs).unwrap(), 1);
        assert!(!exists(&fs, &part), "{suffix}");
    }
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
fn only_a_sole_instance_may_recover() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ferrite.lock");
    let first = super::claim(&file).unwrap();
    assert!(first.sole);
    let first = first.share().unwrap();
    let second = super::claim(&file).unwrap();
    assert!(!second.sole);
    drop(first);
    let third = super::claim(&file).unwrap();
    assert!(!third.sole);
    drop((second, third));
    assert!(super::claim(&file).unwrap().sole);
}

fn run_pausing(
    fs: &FakeFs,
    plan: &OperationPlan,
    journal: &SharedJournal,
    report: Sender<Progress>,
) -> (CancelToken, impl FnOnce() -> JobOutcome) {
    let token = CancelToken::new();
    let cancel = token.clone();
    let mut log = log_factory(Arc::clone(journal))(plan).unwrap();
    let run = move || {
        let mut on_progress = |progress: &Progress| {
            if progress.bytes_done > 0 && !progress.paused {
                cancel.pause();
            }
            let _ = report.send(progress.clone());
        };
        run_job(
            plan,
            &[],
            &mut JobContext {
                ops: fs,
                log: log.as_mut(),
                cancel: &cancel,
                progress: &mut on_progress,
            },
        )
    };
    (token, run)
}

#[test]
fn a_crash_while_paused_mid_copy_is_rolled_back_at_start() {
    let fs = fake();
    fs.add_file("home/docs/video.bin", 10_000);
    let journal = journal();
    let plan = copy_plan(&fs);
    let (sender, seen) = channel();
    let (token, run) = run_pausing(&fs, &plan, &journal, sender);
    let (paused, recovered) = std::thread::scope(|s| {
        let job = s.spawn(run);
        let paused = seen.iter().any(|progress| progress.paused);
        let recovered = recover(&journal, &fs);
        token.cancel();
        let _ = job.join();
        (paused, recovered)
    });
    assert!(paused);
    assert_eq!(recovered.unwrap(), 1);
    assert!(!exists(&fs, "dest/docs"));
    assert!(exists(&fs, "home/docs/video.bin"));
    assert!(lock(&journal).unfinished().unwrap().is_empty());
}
