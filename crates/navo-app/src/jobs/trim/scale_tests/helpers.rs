use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use navo_core::retention::{
    DAY_MS, RetainedKind, RetainedNode, RetainedState, RetentionId, RetentionRecord,
    VAULT_RECORD_VERSION,
};
use navo_core::{FileIdentity, Location};
use navo_store::{Db, JobId, JobState, RetentionCatalog, StepState};

use super::super::*;

pub(super) const MIB: u64 = 1024 * 1024;
pub(super) const PLAN_BYTES: usize = MIB as usize;
pub(super) const MAX_PLAN_BYTES: u64 = 32 * MIB;
pub(super) const SQLITE_OVERHEAD: u64 = 8 * MIB;
pub(super) const LOCK_STEP_BUDGET: Duration = Duration::from_secs(5);
pub(super) const DEFER_BUDGET: Duration = Duration::from_secs(2);
pub(super) const RETAINED_KIND: &str = "delete-retained-v1";

pub(super) struct Fixture {
    _root: tempfile::TempDir,
    pub(super) data: PathBuf,
    pub(super) path: PathBuf,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().to_path_buf();
        let path = data.join("navo.db");
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch("PRAGMA journal_mode = WAL; CREATE TABLE seed (x);")
            .unwrap();
        Self {
            _root: root,
            data,
            path,
        }
    }

    pub(super) fn open(&self) -> SharedJournal {
        crate::jobs::open(&self.path).unwrap()
    }
}

pub(super) struct Survives {
    pub(super) retained: JobId,
    pub(super) retained_entry: RetentionId,
    pub(super) undo: JobId,
    pub(super) undo_row: Vec<u8>,
}

pub(super) struct Holds {
    pub(super) prune: Duration,
    pub(super) vacuum: Duration,
    pub(super) reclaim: Duration,
    pub(super) wal: Duration,
}

pub(super) struct Budgets {
    pub(super) startup: Duration,
    pub(super) runtime: Duration,
}

pub(super) struct Prepared {
    pub(super) journal: SharedJournal,
    pub(super) survives: Survives,
    pub(super) holds: Holds,
    pub(super) before: u64,
    pub(super) newest: JobId,
}

pub(super) fn fill(fixture: &Fixture, jobs: i64, now: i64) -> SharedJournal {
    let journal = fixture.open();
    let mut held = lock(&journal);
    for at in 0..jobs {
        let day = 1 + (jobs - 1 - at) * 89 / jobs;
        let created = now - day * DAY_MS - (at % 24) * 60 * 60 * 1000;
        let job = held.create_job("copy", &big_plan(at), 1, created).unwrap();
        held.finish(job, JobState::Done).unwrap();
    }
    drop(held);
    journal
}

fn big_plan(at: i64) -> Vec<u8> {
    let mut plan = vec![(at % 251) as u8; PLAN_BYTES];
    plan[..8].copy_from_slice(&at.to_le_bytes());
    plan
}

pub(super) fn add_survives(fixture: &Fixture, journal: &SharedJournal, now: i64) -> Survives {
    let retained = {
        let mut held = lock(journal);
        let job = held
            .create_job(RETAINED_KIND, b"retained plan", 1, now - 60 * DAY_MS)
            .unwrap();
        held.finish(job, JobState::Done).unwrap();
        job
    };
    let retained_entry = add_ready_retained(fixture, retained, now);
    let undo_row = b"inverse of the held file".to_vec();
    let undo = {
        let mut held = lock(journal);
        let job = held
            .create_job("copy", b"undo plan", 1, now - 85 * DAY_MS)
            .unwrap();
        held.record_held(job, &undo_row, now - 85 * DAY_MS).unwrap();
        held.finish(job, JobState::Done).unwrap();
        job
    };
    Survives {
        retained,
        retained_entry,
        undo,
        undo_row,
    }
}

fn add_ready_retained(fixture: &Fixture, job: JobId, now: i64) -> RetentionId {
    let (db, _) = Db::open(&fixture.path, false).unwrap();
    let mut catalog = RetentionCatalog::new(db).unwrap();
    let identity = FileIdentity::fingerprint(4 * 1024, Some(now - 2 * DAY_MS), false);
    let record = RetentionRecord {
        version: VAULT_RECORD_VERSION,
        payload_name: None,
        source: Location::new("/sample/docs/report.bin").unwrap(),
        container: Location::new("/sample/.navo-vault/.navo-retained-a1").unwrap(),
        parent_identity: identity,
        container_identity: None,
        nodes: vec![RetainedNode {
            path: Vec::new(),
            kind: RetainedKind::File,
            identity,
            size: 4 * 1024,
            modified_ms: Some(now - 2 * DAY_MS),
        }],
        created_at: now - 2 * DAY_MS,
        expires_at: now + 5 * DAY_MS,
    };
    let mut transaction = catalog.transaction().unwrap();
    let mut entry = transaction.insert(&record, Some(job)).unwrap();
    entry.record.container_identity = Some(identity);
    entry.state = RetainedState::Ready;
    transaction.save(&entry, RetainedState::Preparing).unwrap();
    transaction.commit().unwrap();
    entry.id
}

pub(super) fn old_job(journal: &SharedJournal, created: i64, bytes: usize) -> JobId {
    let mut held = lock(journal);
    let job = held
        .create_job("copy", &vec![7_u8; bytes], 1, created)
        .unwrap();
    held.finish(job, JobState::Done).unwrap();
    job
}

pub(super) fn running_job(journal: &SharedJournal, created: i64) -> JobId {
    let mut held = lock(journal);
    let job = held
        .create_job("copy", &vec![3_u8; MIB as usize], 2, created)
        .unwrap();
    held.mark_step(job, 0, StepState::Done, Some(b"kept"))
        .unwrap();
    job
}

pub(super) fn measure_steps(journal: &SharedJournal, now: i64) -> Holds {
    let started = Instant::now();
    let pruned = lock(journal).prune(PruneLimits::standard(now)).unwrap();
    let prune = started.elapsed();
    assert!(pruned > 0);
    assert!(
        prune < LOCK_STEP_BUDGET,
        "prune held the lock for {prune:?}"
    );

    let started = Instant::now();
    let outcome = lock(journal).vacuum_once(u64::MAX).unwrap();
    let vacuum = started.elapsed();
    assert_eq!(outcome, VacuumOnce::Converted);
    assert!(
        vacuum < LOCK_STEP_BUDGET,
        "vacuum held the lock for {vacuum:?}"
    );

    let mut reclaim = Duration::ZERO;
    for _ in 0..10_000 {
        let started = Instant::now();
        let left = lock(journal).reclaim(1024).unwrap();
        reclaim = reclaim.max(started.elapsed());
        if left == 0 {
            break;
        }
    }

    let started = Instant::now();
    assert!(lock(journal).try_truncate_wal().unwrap());
    let wal = started.elapsed();
    assert!(reclaim < LOCK_STEP_BUDGET, "reclaim held {reclaim:?}");
    assert!(wal < LOCK_STEP_BUDGET, "wal truncate held {wal:?}");
    Holds {
        prune,
        vacuum,
        reclaim,
        wal,
    }
}

pub(super) fn file_bytes(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |metadata| metadata.len())
}

pub(super) fn database_bytes(path: &Path) -> u64 {
    let mut bytes = file_bytes(path);
    let wal = sidecar(path, "-wal");
    bytes += file_bytes(&wal);
    bytes
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}
