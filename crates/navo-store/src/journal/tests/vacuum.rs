use std::path::Path;

use super::*;

const PLAN: usize = 256 * 1024;

fn old_database(path: &Path) {
    let raw = Connection::open(path).unwrap();
    raw.execute_batch("PRAGMA journal_mode = WAL; CREATE TABLE seed (x);")
        .unwrap();
}

fn open(path: &Path) -> JobJournal {
    JobJournal::new(Db::open(path, false).unwrap().0).unwrap()
}

fn fill(journal: &mut JobJournal, jobs: i64) -> Vec<JobId> {
    (0..jobs)
        .map(|at| {
            let job = journal
                .create_job("copy", &vec![9_u8; PLAN], 2, at)
                .unwrap();
            journal.finish(job, JobState::Done).unwrap();
            job
        })
        .collect()
}

fn pragma(journal: &JobJournal, name: &str) -> i64 {
    conn(journal)
        .pragma_query_value(None, name, |row| row.get(0))
        .unwrap()
}

fn integrity(journal: &JobJournal) -> String {
    conn(journal)
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn a_new_database_starts_with_incremental_vacuum() {
    let dir = tempfile::tempdir().unwrap();
    let journal = open(&dir.path().join("navo.db"));
    assert_eq!(pragma(&journal, "auto_vacuum"), 2);
    assert!(
        Db::open_in_memory()
            .unwrap()
            .is_incremental_vacuum()
            .unwrap()
    );
}

#[test]
fn an_older_database_is_converted_once_and_keeps_its_jobs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("navo.db");
    old_database(&path);
    let mut journal = open(&path);
    let jobs = fill(&mut journal, 3);
    assert_eq!(pragma(&journal, "auto_vacuum"), 0);
    assert_eq!(
        journal.vacuum_once(u64::MAX).unwrap(),
        VacuumOnce::Converted
    );
    assert_eq!(
        journal.vacuum_once(u64::MAX).unwrap(),
        VacuumOnce::AlreadyIncremental
    );
    drop(journal);
    let journal = open(&path);
    assert_eq!(pragma(&journal, "auto_vacuum"), 2);
    assert_eq!(integrity(&journal), "ok");
    assert_eq!(count(&journal, "SELECT count(*) FROM job"), 3);
    assert_eq!(journal.read_job(jobs[2]).unwrap().1.plan.len(), PLAN);
}

#[test]
fn the_conversion_waits_while_a_job_is_running_or_waits_for_a_restore() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("navo.db");
    old_database(&path);
    let mut journal = open(&path);
    let running = journal.create_job("copy", b"plan", 1, 1).unwrap();
    assert_eq!(
        journal.vacuum_once(u64::MAX).unwrap(),
        VacuumOnce::JobsPending
    );
    journal.finish(running, JobState::RestorePending).unwrap();
    assert_eq!(
        journal.vacuum_once(u64::MAX).unwrap(),
        VacuumOnce::JobsPending
    );
    assert_eq!(pragma(&journal, "auto_vacuum"), 0);
    journal.finish(running, JobState::Done).unwrap();
    assert_eq!(
        journal.vacuum_once(u64::MAX).unwrap(),
        VacuumOnce::Converted
    );
}

#[test]
fn the_conversion_needs_twice_the_database_size_free() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("navo.db");
    old_database(&path);
    let mut journal = open(&path);
    fill(&mut journal, 4);
    let size =
        u64::try_from(pragma(&journal, "page_count") * pragma(&journal, "page_size")).unwrap();
    assert_eq!(
        journal.vacuum_once(size).unwrap(),
        VacuumOnce::LowSpace { needed: size * 2 }
    );
    assert_eq!(pragma(&journal, "auto_vacuum"), 0);
}

#[test]
fn an_interrupted_conversion_leaves_a_consistent_database_for_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("navo.db");
    old_database(&path);
    let mut journal = open(&path);
    for at in 0..2000 {
        journal.create_job("copy", b"plan", 2, at).unwrap();
        journal.finish(JobId(at + 1), JobState::Done).unwrap();
    }
    let calls = std::sync::atomic::AtomicU32::new(0);
    conn(&journal)
        .progress_handler(
            10,
            Some(move || calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed) > 200),
        )
        .unwrap();
    let error = journal.db.vacuum_incremental().unwrap_err();
    assert!(error.to_string().contains("interrupt"), "{error}");
    drop(journal);
    let mut journal = open(&path);
    assert_eq!(integrity(&journal), "ok");
    assert_eq!(pragma(&journal, "auto_vacuum"), 0);
    assert_eq!(count(&journal, "SELECT count(*) FROM job"), 2000);
    assert_eq!(
        journal.vacuum_once(u64::MAX).unwrap(),
        VacuumOnce::Converted
    );
    assert_eq!(integrity(&journal), "ok");
}

#[test]
fn prune_then_reclaim_shrinks_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("navo.db");
    let mut journal = open(&path);
    fill(&mut journal, 40);
    assert!(journal.try_truncate_wal().unwrap());
    let before = pragma(&journal, "page_count");
    journal.prune(PruneLimits::keep(5)).unwrap();
    assert!(pragma(&journal, "freelist_count") > 0);
    while journal.reclaim(64).unwrap() > 0 {}
    assert!(journal.try_truncate_wal().unwrap());
    let after = pragma(&journal, "page_count");
    assert_eq!(pragma(&journal, "freelist_count"), 0);
    assert!(after * 4 < before, "pages {before} -> {after}");
    let file = std::fs::metadata(&path).unwrap().len();
    let page = u64::try_from(pragma(&journal, "page_size")).unwrap();
    assert_eq!(file, u64::try_from(after).unwrap() * page);
}

#[test]
fn reclaim_does_nothing_on_a_database_not_yet_converted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("navo.db");
    old_database(&path);
    let mut journal = open(&path);
    fill(&mut journal, 4);
    journal.prune(PruneLimits::keep(1)).unwrap();
    assert_eq!(journal.reclaim(1000).unwrap(), 0);
    assert!(pragma(&journal, "freelist_count") > 0);
}

#[test]
fn an_idle_prune_and_reclaim_skip_at_once_while_another_connection_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("navo.db");
    let mut journal = open(&path);
    fill(&mut journal, 10);
    let other = Connection::open(&path).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = std::time::Instant::now();
    assert_eq!(journal.prune_if_idle(PruneLimits::keep(2)).unwrap(), None);
    assert_eq!(journal.reclaim_if_idle(64).unwrap(), None);
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert_eq!(pragma(&journal, "busy_timeout"), 5000);
    other.execute_batch("ROLLBACK").unwrap();
    assert!(
        journal
            .prune_if_idle(PruneLimits::keep(2))
            .unwrap()
            .is_some()
    );
    assert!(journal.reclaim_if_idle(64).unwrap().is_some());
}
