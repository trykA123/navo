use std::path::Path;
use std::time::{Duration, Instant};

use navo_core::retention::DAY_MS;
use navo_fs::LocalFs;
use navo_store::{Db, JobId, JobState, RetentionCatalog};

use super::*;
use helpers::*;

mod helpers;

#[test]
fn a_twenty_megabyte_journal_trims_and_keeps_recovery_rows() {
    run_scale(
        "20 MiB journal",
        20,
        20 * MIB,
        Budgets {
            startup: Duration::from_secs(10),
            runtime: Duration::from_secs(5),
        },
    );
}

#[test]
#[ignore = "scale: builds about 220 MiB of plans; run explicitly with --run-ignored ignored-only"]
fn a_two_hundred_twenty_megabyte_journal_trims_within_budget() {
    run_scale(
        "220 MiB journal",
        220,
        220 * MIB,
        Budgets {
            startup: Duration::from_secs(30),
            runtime: Duration::from_secs(10),
        },
    );
}

#[allow(
    clippy::print_stderr,
    reason = "the scale test reports the measured sizes and times"
)]
fn run_scale(label: &str, jobs: i64, plan_bytes: u64, budgets: Budgets) {
    let fixture = Fixture::new();
    let now = now_ms();
    let prepared = prepare(&fixture, jobs, plan_bytes, now);
    let (after, startup) = first_start(&fixture, &prepared, &budgets);
    let second_start = second_start(&fixture, &prepared.journal, &budgets);
    let (after_runtime, runtime) = runtime_trim(&fixture, &prepared.journal, &budgets);
    assert_survives(&prepared.journal, &fixture.path, &prepared.survives);
    assert_deferred_while_busy(&prepared.journal, &fixture.path);
    eprintln!(
        "{label}: {} -> {after_runtime} bytes; startup {startup:?}; second start \
         {second_start:?}; runtime trim {runtime:?}; lock holds prune {:?}, vacuum {:?}, \
         reclaim {:?}, wal {:?}",
        prepared.before,
        prepared.holds.prune,
        prepared.holds.vacuum,
        prepared.holds.reclaim,
        prepared.holds.wal
    );
    assert!(after <= after_runtime);
}

fn prepare(fixture: &Fixture, jobs: i64, plan_bytes: u64, now: i64) -> Prepared {
    let journal = fill(fixture, jobs, now);
    let survives = add_survives(fixture, &journal, now);
    assert!(!lock(&journal).database().is_incremental_vacuum().unwrap());
    lock(&journal).database().checkpoint().unwrap();
    drop(journal);
    let measured = fixture.data.join("measured.db");
    std::fs::copy(&fixture.path, &measured).unwrap();
    let journal = fixture.open();
    let before = file_bytes(&fixture.path);
    assert!(before >= plan_bytes, "database only {before} bytes");
    let holds = measure_steps(&crate::jobs::open(&measured).unwrap(), now);
    Prepared {
        journal,
        survives,
        holds,
        before,
        newest: JobId(jobs),
    }
}

fn first_start(fixture: &Fixture, prepared: &Prepared, budgets: &Budgets) -> (u64, Duration) {
    let started = Instant::now();
    at_startup(&prepared.journal, &fixture.data, &LocalFs::new()).unwrap();
    let startup = started.elapsed();
    assert!(
        startup < budgets.startup,
        "startup took {startup:?}, budget {:?}",
        budgets.startup
    );
    let after = file_bytes(&fixture.path);
    let cap = MAX_PLAN_BYTES + SQLITE_OVERHEAD;
    let (jobs_left, plan_left, steps_left): (i64, i64, i64) = lock(&prepared.journal)
        .database()
        .connection()
        .query_row(
            "SELECT (SELECT count(*) FROM job),
                (SELECT CAST(total(length(plan)) AS INTEGER) FROM job),
                (SELECT CAST(total(length(detail)) AS INTEGER) FROM job_step)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert!(
        plan_left <= MAX_PLAN_BYTES as i64 + 1024,
        "surviving plans {plan_left} in {jobs_left} jobs, steps {steps_left}, file {after}"
    );
    assert!(after < cap, "journal is {after} bytes, cap {cap}");
    assert!(
        after < prepared.before,
        "journal did not shrink: {} -> {after}",
        prepared.before
    );
    assert!(
        lock(&prepared.journal)
            .database()
            .is_incremental_vacuum()
            .unwrap()
    );
    assert!(lock(&prepared.journal).read_job(JobId(1)).is_err());
    assert_eq!(
        lock(&prepared.journal)
            .read_job(prepared.newest)
            .unwrap()
            .1
            .plan
            .len(),
        PLAN_BYTES
    );
    (after, startup)
}

fn second_start(fixture: &Fixture, journal: &SharedJournal, budgets: &Budgets) -> Duration {
    let extra = old_job(journal, now_ms() - 70 * DAY_MS, 2 * MIB as usize);
    let running = running_job(journal, now_ms() - 80 * DAY_MS);
    let started = Instant::now();
    at_startup(journal, &fixture.data, &LocalFs::new()).unwrap();
    let elapsed = started.elapsed();
    assert!(elapsed < budgets.startup);
    assert!(file_bytes(&fixture.path) < MAX_PLAN_BYTES + SQLITE_OVERHEAD);
    assert!(!exists(journal, extra));
    assert!(lock(journal).read_job(running).is_ok());
    assert_second_start_skips_vacuum(journal, &fixture.path);
    elapsed
}

fn runtime_trim(fixture: &Fixture, journal: &SharedJournal, budgets: &Budgets) -> (u64, Duration) {
    let extra = old_job(journal, now_ms() - 65 * DAY_MS, 8 * MIB as usize);
    let newest = old_job(journal, now_ms(), 4 * 1024);
    let before = database_bytes(&fixture.path);
    let started = Instant::now();
    let outcome = trim(journal);
    let elapsed = started.elapsed();
    assert!(
        matches!(outcome, Trim::Done { pruned } if pruned >= 1),
        "runtime trim returned {outcome:?}"
    );
    assert!(
        elapsed < budgets.runtime,
        "runtime trim took {elapsed:?}, budget {:?}",
        budgets.runtime
    );
    assert!(!exists(journal, extra));
    assert!(exists(journal, newest));
    let after = database_bytes(&fixture.path);
    assert!(
        after < before,
        "runtime trim did not shrink: {before} -> {after}"
    );
    assert!(after < MAX_PLAN_BYTES + SQLITE_OVERHEAD);
    (after, elapsed)
}

fn exists(journal: &SharedJournal, job: JobId) -> bool {
    lock(journal).read_job(job).is_ok()
}

fn assert_survives(journal: &SharedJournal, path: &Path, survives: &Survives) {
    let (state, stored) = lock(journal).read_job(survives.retained).unwrap();
    assert_eq!(state, JobState::Done);
    assert_eq!(stored.kind, RETAINED_KIND);
    assert_eq!(stored.plan.as_slice(), b"retained plan");
    assert_eq!(
        lock(journal)
            .retained_paths(survives.retained)
            .unwrap()
            .len(),
        2
    );
    let (db, _) = Db::open(path, false).unwrap();
    let catalog = RetentionCatalog::new(db).unwrap();
    let entry = catalog.get(survives.retained_entry).unwrap();
    assert_eq!(entry.state, navo_core::retention::RetainedState::Ready);
    assert_eq!(entry.job_id, Some(survives.retained.0));

    let (state, stored) = lock(journal).read_job(survives.undo).unwrap();
    assert_eq!(state, JobState::Done);
    assert_eq!(stored.plan.as_slice(), b"undo plan");
    assert_eq!(
        lock(journal).held(survives.undo).unwrap(),
        vec![survives.undo_row.clone()]
    );
}

fn assert_second_start_skips_vacuum(journal: &SharedJournal, path: &Path) {
    let reader = rusqlite::Connection::open(path).unwrap();
    reader
        .execute_batch("BEGIN; SELECT count(*) FROM job;")
        .unwrap();
    let started = Instant::now();
    assert_eq!(
        lock(journal).vacuum_once(u64::MAX).unwrap(),
        VacuumOnce::AlreadyIncremental
    );
    assert!(started.elapsed() < DEFER_BUDGET);
    drop(reader);
    assert!(lock(journal).database().is_incremental_vacuum().unwrap());
}

fn assert_deferred_while_busy(journal: &SharedJournal, path: &Path) {
    let blocker = rusqlite::Connection::open(path).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = Instant::now();
    assert_eq!(trim(journal), Trim::Deferred);
    assert!(started.elapsed() < DEFER_BUDGET);
    blocker.execute_batch("ROLLBACK").unwrap();

    let held = lock(journal);
    let started = Instant::now();
    assert_eq!(trim(journal), Trim::Deferred);
    assert!(started.elapsed() < DEFER_BUDGET);
    drop(held);
}
