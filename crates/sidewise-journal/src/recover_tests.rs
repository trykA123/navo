//! Tests of recovery at open (J3): roll back a dead owner's open job, skip a live owner's,
//! leave a damaged log alone, close a torn log by rewrite (NC164), settle the rewrite's
//! leftovers, keep a commit in doubt that is durable (NC162), and remove dead owners' locks.
// Linux only: `RealDisk` is built on Linux until D3.
#![cfg(target_os = "linux")]

use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use sidewise_fs::durable::{FaultDisk, Plan, RealDisk, Tail};

use crate::job_tests::{Fixture, NEW, OLD, OLD_MS, mtime_ms, only_log, run, write_new};
use crate::recover_crash_tests::check_settled;
use crate::{Expect, ItemMode, JobId, JobKind, Journal, Record, Recovered, Swept, encode};

/// Writes `NEW` over the first `count` targets in one job, then loses power with every change
/// kept, so the job stays open and its owner is dead.
pub(crate) fn crashed_open_job(fixture: &Fixture, count: usize) {
    let disk = Arc::new(FaultDisk::new(RealDisk, Plan::Pass));
    let journal = Journal::open_raw(&fixture.journal_dir(), disk.clone()).unwrap();
    let mut job = journal.begin(JobKind::CopyMany, "Copy", None).unwrap();
    for target in fixture.targets.iter().take(count) {
        job.overwrite(target, Expect::default(), None, &mut write_new)
            .unwrap();
    }
    disk.crash_with(|_| true, |_| Tail::Kept).unwrap();
}

/// Opens the journal with `RealDisk` and returns what recovery did, sweep counts zeroed
/// (`sweep_tests` checks them).
fn open(fixture: &Fixture) -> (Journal, Recovered) {
    let (journal, recovered) = Journal::open(&fixture.journal_dir(), Arc::new(RealDisk)).unwrap();
    let swept = Swept::default();
    (journal, Recovered { swept, ..recovered })
}

/// The log path of the only job.
fn log_path(fixture: &Fixture) -> PathBuf {
    fixture.jobs()[0].join("log")
}

/// The names in `v1/owners`.
fn owner_files(fixture: &Fixture) -> Vec<String> {
    let names = fs::read_dir(fixture.journal_dir().join("v1/owners")).unwrap();
    names
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect()
}

#[test]
fn recover_rolls_back_an_open_job_of_a_dead_owner() {
    let fixture = Fixture::new(2);
    crashed_open_job(&fixture, 2);
    assert_eq!(fs::read(&fixture.targets[0]).unwrap(), NEW);
    let (journal, recovered) = open(&fixture);
    let want = Recovered {
        restored: 2,
        rolled_back: 1,
        ..Recovered::default()
    };
    assert_eq!(recovered, want);
    check_settled(&fixture, "open job").unwrap();
    let last = only_log(&fixture).records.pop();
    let reason = "recovered".to_owned();
    assert_eq!(last, Some(Record::Aborted { reason }));
    assert_eq!(owner_files(&fixture), [format!("{}.lock", journal.owner())]);
}

/// A job killed before any target changed restores nothing, yet recovery still closes it, so
/// it counts once as rolled back (the start-up note relies on it).
#[test]
fn recover_counts_an_open_job_that_changed_no_target_as_rolled_back() {
    let fixture = Fixture::new(1);
    crashed_open_job(&fixture, 0);
    let (_journal, recovered) = open(&fixture);
    let want = Recovered {
        rolled_back: 1,
        ..Recovered::default()
    };
    assert_eq!(recovered, want);
    let last = only_log(&fixture).records.pop();
    let reason = "recovered".to_owned();
    assert_eq!(last, Some(Record::Aborted { reason }));
    assert_eq!(
        fs::read(&fixture.targets[0]).unwrap(),
        OLD,
        "target untouched"
    );
}

#[test]
fn recover_skips_a_job_whose_owner_lock_is_held() {
    let fixture = Fixture::new(1);
    let live = Journal::open_raw(&fixture.journal_dir(), Arc::new(RealDisk)).unwrap();
    let mut job = live.begin(JobKind::Copy, "Copy", None).unwrap();
    job.overwrite(&fixture.targets[0], Expect::default(), None, &mut write_new)
        .unwrap();
    let before = fs::read(log_path(&fixture)).unwrap();
    let (_second, recovered) = open(&fixture);
    assert_eq!(recovered, Recovered::default());
    assert_eq!(fs::read(&fixture.targets[0]).unwrap(), NEW, "live job kept");
    assert_eq!(
        fs::read(log_path(&fixture)).unwrap(),
        before,
        "log untouched"
    );
    assert!(owner_files(&fixture).contains(&format!("{}.lock", live.owner())));
    job.commit().unwrap();
}

#[test]
fn recover_reports_a_damaged_log_and_leaves_it_alone() {
    let fixture = Fixture::new(1);
    crashed_open_job(&fixture, 1);
    let mut bytes = fs::read(log_path(&fixture)).unwrap();
    bytes[5] ^= 0x01;
    fs::write(log_path(&fixture), &bytes).unwrap();
    let (_journal, recovered) = open(&fixture);
    let want = Recovered {
        damaged: 1,
        ..Recovered::default()
    };
    assert_eq!(recovered, want);
    assert_eq!(
        fs::read(log_path(&fixture)).unwrap(),
        bytes,
        "log untouched"
    );
    assert_eq!(
        fs::read(&fixture.targets[0]).unwrap(),
        NEW,
        "target untouched"
    );
    assert_eq!(fs::read(fixture.jobs()[0].join("0.bak")).unwrap(), OLD);
}

#[test]
fn recover_closes_a_torn_log_by_an_atomic_rewrite() {
    let fixture = Fixture::new(1);
    crashed_open_job(&fixture, 1);
    let good = fs::read(log_path(&fixture)).unwrap();
    let mut torn = good.clone();
    torn.extend_from_slice(b"{\"t\":\"Do");
    fs::write(log_path(&fixture), &torn).unwrap();
    let (_journal, recovered) = open(&fixture);
    assert_eq!(recovered.restored, 1);
    let mut want = good;
    let reason = "recovered".to_owned();
    want.extend_from_slice(encode(&Record::Aborted { reason }).as_bytes());
    assert_eq!(fs::read(log_path(&fixture)).unwrap(), want);
    check_settled(&fixture, "torn log").unwrap();
}

#[test]
fn recover_puts_back_a_log_moved_aside_by_a_failed_replace() {
    let fixture = Fixture::new(1);
    crashed_open_job(&fixture, 1);
    let job = fixture.jobs()[0].clone();
    fs::rename(job.join("log"), job.join("log.sw-old")).unwrap();
    fs::write(job.join("log.sw-new"), b"half a rewrite").unwrap();
    let (_journal, recovered) = open(&fixture);
    assert_eq!(recovered.restored, 1);
    check_settled(&fixture, "log moved aside").unwrap();
    assert!(
        only_log(&fixture).records.len() > 1,
        "the old records are kept"
    );
}

#[test]
fn recover_deletes_a_leftover_rewrite_beside_a_closed_log() {
    let fixture = Fixture::new(1);
    run(Arc::new(RealDisk), &fixture, true).unwrap();
    let job = fixture.jobs()[0].clone();
    fs::write(job.join("log.sw-new"), b"half a rewrite").unwrap();
    let (_journal, recovered) = open(&fixture);
    assert_eq!(recovered, Recovered::default());
    check_settled(&fixture, "leftover rewrite").unwrap();
}

#[test]
fn recover_keeps_the_new_bytes_of_a_durable_commit_in_doubt() {
    let fixture = Fixture::new(2);
    let counter = Arc::new(FaultDisk::new(RealDisk, Plan::Pass));
    run(counter.clone(), &Fixture::new(2), true).unwrap();
    // The last op of a committed run is the sync of the `Commit` line.
    let plan = Plan::FailAt(counter.ops(), io::ErrorKind::Other);
    let error = run(Arc::new(FaultDisk::new(RealDisk, plan)), &fixture, true).unwrap_err();
    assert!(error.contains("in doubt"), "{error}");
    let (_journal, recovered) = open(&fixture);
    assert_eq!(recovered, Recovered::default());
    assert_eq!(only_log(&fixture).records.last(), Some(&Record::Commit));
    check_settled(&fixture, "commit in doubt").unwrap();
    assert_eq!(fs::read(&fixture.targets[1]).unwrap(), NEW);
}

#[test]
fn recover_removes_the_locks_of_dead_owners_with_complete_jobs() {
    let fixture = Fixture::new(1);
    run(Arc::new(RealDisk), &fixture, true).unwrap();
    run(Arc::new(RealDisk), &fixture, false).unwrap_err();
    assert_eq!(owner_files(&fixture).len(), 2);
    let (journal, _) = open(&fixture);
    assert_eq!(owner_files(&fixture), [format!("{}.lock", journal.owner())]);
}

#[test]
fn recover_treats_a_relative_target_as_damage() {
    let fixture = Fixture::new(1);
    let id = JobId::new(1_700_000_000_000, 1, 0);
    let job = fixture.journal_dir().join("v1").join(id.as_str());
    fs::create_dir_all(&job).unwrap();
    let begin = Record::Begin {
        job: id,
        kind: JobKind::Copy,
        label: "Copy".into(),
        created_ms: 1_700_000_000_000,
        reverts: None,
        owner: "1-1".into(),
    };
    let intent = Record::Intent {
        item: 0,
        target: "files/0.txt".into(),
        mode: ItemMode::Overwrite,
        expect_size: None,
        expect_mtime_ms: None,
        restore_mtime_ms: None,
    };
    let bytes = encode(&begin) + &encode(&intent);
    fs::write(job.join("log"), &bytes).unwrap();
    let (_journal, recovered) = open(&fixture);
    assert_eq!(recovered.damaged, 1);
    assert_eq!(
        fs::read(job.join("log")).unwrap(),
        bytes.as_bytes(),
        "log untouched"
    );
    assert_eq!(
        (
            fs::read(&fixture.targets[0]).unwrap(),
            mtime_ms(&fixture.targets[0])
        ),
        (OLD.to_vec(), OLD_MS)
    );
}
