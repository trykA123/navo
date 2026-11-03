use std::time::{Duration, Instant};

use navo_core::{ConflictPolicy, OpKind, PlanRequest, plan};
use navo_fs::{CancelToken, FakeFs, JobOutcome, LocalFs, scan};
use navo_store::{Db, JobId, JobState};

use super::*;
use crate::jobs::log_factory;

fn journal() -> SharedJournal {
    Arc::new(Mutex::new(
        JobJournal::new(Db::open_in_memory().unwrap()).unwrap(),
    ))
}

fn old_job(journal: &SharedJournal) -> JobId {
    let mut held = lock(journal);
    let job = held.create_job("copy", b"plan", 1, 1).unwrap();
    held.finish(job, JobState::Done).unwrap();
    let newest = held.create_job("copy", b"plan", 1, now_ms()).unwrap();
    held.finish(newest, JobState::Done).unwrap();
    job
}

fn exists(journal: &SharedJournal, job: JobId) -> bool {
    lock(journal).read_job(job).is_ok()
}

fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::yield_now();
    }
    false
}

fn finish_a_job(journal: &SharedJournal) {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_folder("dest");
    let plan = plan(&PlanRequest {
        kind: OpKind::Copy,
        items: scan(&fs, &[fs.location("home/docs")], &CancelToken::new()).unwrap(),
        target: Some(fs.location("dest")),
        existing: Vec::new(),
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: Vec::new(),
    });
    let mut log = log_factory(Arc::clone(journal))(&plan).unwrap();
    log.finish(&JobOutcome::Done, &fs).unwrap();
}

#[test]
fn a_finished_job_trims_the_journal_in_the_background() {
    let journal = journal();
    let old = old_job(&journal);
    arm(&journal);
    finish_a_job(&journal);
    assert!(wait_until(|| !exists(&journal, old)));
}

#[test]
fn recovery_defers_the_trim_until_it_ends() {
    let journal = journal();
    let old = old_job(&journal);
    let guard = recovering(&journal);
    assert_eq!(trim(&journal), Trim::Deferred);
    arm(&journal);
    after_job(&journal);
    assert!(exists(&journal, old));
    drop(guard);
    assert!(wait_until(|| !exists(&journal, old)));
}

#[test]
fn a_held_journal_defers_the_trim() {
    let journal = journal();
    let old = old_job(&journal);
    let held = lock(&journal);
    assert_eq!(trim(&journal), Trim::Deferred);
    drop(held);
    assert_eq!(trim(&journal), Trim::Done { pruned: 1 });
    assert!(!exists(&journal, old));
}

#[test]
fn startup_converts_an_older_journal_to_incremental_vacuum() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("navo.db");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("PRAGMA journal_mode = WAL; CREATE TABLE seed (x);")
        .unwrap();
    let journal = crate::jobs::open(&path).unwrap();
    let old = old_job(&journal);
    at_startup(&journal, dir.path(), &LocalFs::new()).unwrap();
    assert!(!exists(&journal, old));
    assert!(lock(&journal).database().is_incremental_vacuum().unwrap());
}

#[test]
fn a_database_written_elsewhere_skips_the_runtime_trim_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("navo.db");
    let journal = crate::jobs::open(&path).unwrap();
    let old = old_job(&journal);
    let other = rusqlite::Connection::open(&path).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = Instant::now();
    assert_eq!(trim(&journal), Trim::Deferred);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(exists(&journal, old));
    other.execute_batch("ROLLBACK").unwrap();
    assert_eq!(trim(&journal), Trim::Done { pruned: 1 });
}
