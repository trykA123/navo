//! Tests of what a failed job tells its caller (review fixes of J2): an overwrite error says
//! when the rollback did not finish, a commit whose sync failed is `InDoubt` (NC162), and the
//! owner lock survives another process's recovery racing its create.
// Linux only: `RealDisk` is built on Linux until D3.
#![cfg(target_os = "linux")]

use std::fs::{self, File};
use std::io::ErrorKind;
use std::sync::Arc;

use sidewise_fs::durable::{Disk, FaultDisk, Plan, RealDisk};
use sidewise_fs::native::identity;

use crate::job_tests::{Fixture, NEW, OLD, OLD_MS, mtime_ms, write_new};
use crate::journal::lock_owner_with;
use crate::{Expect, JobKind, Journal, JournalError, Record, WriteError};

/// Opens a journal on `disk`, starts a job and overwrites the one target; `None` when the
/// journal or the job could not start.
fn overwrite_one(disk: Arc<dyn Disk>, fixture: &Fixture) -> Option<Result<u32, WriteError>> {
    let journal = Journal::open_raw(&fixture.journal_dir(), disk).ok()?;
    let mut job = journal.begin(JobKind::Copy, "Copy", None).ok()?;
    Some(job.overwrite(&fixture.targets[0], Expect::default(), None, &mut write_new))
}

#[test]
fn job_overwrite_error_names_a_rollback_that_did_not_finish() {
    // The ops up to the end of `overwrite`, before the drop's rollback.
    let ops = {
        let fixture = Fixture::new(1);
        let disk = Arc::new(FaultDisk::new(RealDisk, Plan::Pass));
        let journal = Journal::open_raw(&fixture.journal_dir(), disk.clone()).unwrap();
        let mut job = journal.begin(JobKind::Copy, "Copy", None).unwrap();
        job.overwrite(&fixture.targets[0], Expect::default(), None, &mut write_new)
            .unwrap();
        disk.ops()
    };
    let mut new_seen = false;
    for n in 1..=ops {
        let fixture = Fixture::new(1);
        let plan = Plan::FailFrom(n, ErrorKind::StorageFull);
        let Some(result) = overwrite_one(Arc::new(FaultDisk::new(RealDisk, plan)), &fixture) else {
            continue;
        };
        let target = &fixture.targets[0];
        let bytes = fs::read(target).unwrap();
        match result {
            Err(WriteError::NotRolledBack { cause, failed }) => {
                assert_eq!((*cause, failed), (WriteError::Full, 1), "op {n}");
                new_seen |= bytes == NEW;
            }
            Err(error) => {
                assert_eq!(bytes, OLD, "op {n}: {error:?} but the target is not old");
                assert_eq!(mtime_ms(target), OLD_MS, "op {n}: old time");
            }
            Ok(item) => panic!("op {n}: Ok({item}) with a full disk"),
        }
    }
    assert!(new_seen, "a full disk on the Done append leaves new bytes");
}

/// Runs a one-item job to its commit on a disk that fails op `n` once.
fn commit_failing_at(n: u64, fixture: &Fixture) -> Result<(), JournalError> {
    let disk = Arc::new(FaultDisk::new(RealDisk, Plan::FailAt(n, ErrorKind::Other)));
    let journal = Journal::open_raw(&fixture.journal_dir(), disk).unwrap();
    let mut job = journal.begin(JobKind::Copy, "Copy", None).unwrap();
    job.overwrite(&fixture.targets[0], Expect::default(), None, &mut write_new)
        .unwrap();
    job.commit().map(|_| ())
}

#[test]
fn job_commit_sync_failure_is_in_doubt_with_new_bytes() {
    let ops = {
        let fixture = Fixture::new(1);
        let disk = Arc::new(FaultDisk::new(RealDisk, Plan::Pass));
        let journal = Journal::open_raw(&fixture.journal_dir(), disk.clone()).unwrap();
        let mut job = journal.begin(JobKind::Copy, "Copy", None).unwrap();
        job.overwrite(&fixture.targets[0], Expect::default(), None, &mut write_new)
            .unwrap();
        job.commit().unwrap();
        disk.ops()
    };
    // The last op is the sync of the Commit line, the one before it its write.
    let fixture = Fixture::new(1);
    let result = commit_failing_at(ops, &fixture);
    assert!(
        matches!(result, Err(JournalError::InDoubt(_))),
        "{result:?}"
    );
    assert_eq!(fs::read(&fixture.targets[0]).unwrap(), NEW);
    let log = crate::job_tests::only_log(&fixture);
    assert_eq!(log.records.last(), Some(&Record::Commit));

    let fixture = Fixture::new(1);
    let result = commit_failing_at(ops - 1, &fixture);
    assert!(matches!(result, Err(JournalError::Io(_))), "{result:?}");
    assert_eq!(fs::read(&fixture.targets[0]).unwrap(), OLD);
}

#[test]
fn job_commit_line_failing_for_good_is_in_doubt_when_the_rollback_fails_too() {
    let ops = {
        let fixture = Fixture::new(1);
        let disk = Arc::new(FaultDisk::new(RealDisk, Plan::Pass));
        let journal = Journal::open_raw(&fixture.journal_dir(), disk.clone()).unwrap();
        let mut job = journal.begin(JobKind::Copy, "Copy", None).unwrap();
        job.overwrite(&fixture.targets[0], Expect::default(), None, &mut write_new)
            .unwrap();
        job.commit().unwrap();
        disk.ops()
    };
    // From the write of the Commit line on, every op fails, so the rollback fails too.
    let fixture = Fixture::new(1);
    let plan = Plan::FailFrom(ops - 1, ErrorKind::Other);
    let disk = Arc::new(FaultDisk::new(RealDisk, plan));
    let journal = Journal::open_raw(&fixture.journal_dir(), disk).unwrap();
    let mut job = journal.begin(JobKind::Copy, "Copy", None).unwrap();
    job.overwrite(&fixture.targets[0], Expect::default(), None, &mut write_new)
        .unwrap();
    let result = job.commit();
    assert!(
        matches!(result, Err(JournalError::InDoubt(_))),
        "{result:?}"
    );
    assert_eq!(fs::read(&fixture.targets[0]).unwrap(), NEW);
}

/// Locks an owner while `race` acts, at the first name only, as another process's recovery.
fn lock_with_race(race: impl Fn(&std::path::Path) -> Option<File>) -> (String, File) {
    let fixture = Fixture::new(0);
    let owners = fixture.journal_dir();
    fs::create_dir_all(&owners).unwrap();
    let mut held = Vec::new();
    let mut first = true;
    let (owner, file) = lock_owner_with(&owners, 1_000, |path| {
        if std::mem::take(&mut first) {
            held.extend(race(path));
        }
    })
    .unwrap();
    let path = owners.join(format!("{owner}.lock"));
    assert_eq!(
        identity::from_file(&file).unwrap(),
        identity::read(&path).unwrap(),
        "the held lock is the file named {owner}"
    );
    drop(fixture);
    (owner, file)
}

#[test]
fn owner_lock_deleted_before_the_lock_takes_a_new_name() {
    let (owner, _) = lock_with_race(|path| {
        fs::remove_file(path).unwrap();
        None
    });
    assert!(owner.ends_with("-1001"), "{owner}");
}

#[test]
fn owner_lock_replaced_before_the_lock_takes_a_new_name() {
    let (owner, _) = lock_with_race(|path| {
        fs::remove_file(path).unwrap();
        fs::write(path, b"").unwrap();
        None
    });
    assert!(owner.ends_with("-1001"), "{owner}");
}

#[test]
fn owner_lock_held_by_a_recovery_takes_a_new_name() {
    let (owner, _) = lock_with_race(|path| {
        let probe = File::open(path).unwrap();
        probe.try_lock().unwrap();
        Some(probe)
    });
    assert!(owner.ends_with("-1001"), "{owner}");
}
