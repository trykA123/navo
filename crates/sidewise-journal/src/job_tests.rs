//! Tests of the job writer (J2): the happy path, refusals and changes before any write, the
//! owner lock, abort and drop, and the `FailAt` fault loop over every op of a two-item job.
// Linux only: `RealDisk` is built on Linux until D3.
#![cfg(target_os = "linux")]

use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use sidewise_fs::durable::{Disk, FaultDisk, Plan, RealDisk};

use crate::{Expect, JobKind, Journal, Record, WriteError, decode};

pub(crate) const OLD: &[u8] = b"old bytes\n";
pub(crate) const NEW: &[u8] = b"the new bytes, longer\n";
/// The old modified time of every target, in ms since the Unix epoch.
pub(crate) const OLD_MS: i64 = 1_700_000_000_000;

/// A temp folder with `files/<i>.txt` targets holding `OLD` at `OLD_MS`, and a journal folder.
pub(crate) struct Fixture {
    pub(crate) dir: tempfile::TempDir,
    pub(crate) targets: Vec<PathBuf>,
}

impl Fixture {
    pub(crate) fn new(count: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("files")).unwrap();
        let targets = (0..count)
            .map(|i| dir.path().join("files").join(format!("{i}.txt")))
            .collect();
        let fixture = Self { dir, targets };
        for target in &fixture.targets {
            fs::write(target, OLD).unwrap();
            set_mtime(target, OLD_MS);
        }
        fixture
    }

    pub(crate) fn journal_dir(&self) -> PathBuf {
        self.dir.path().join("journal")
    }

    /// The names in `files/` that end in the temp suffix.
    pub(crate) fn temps(&self) -> Vec<String> {
        let names = fs::read_dir(self.dir.path().join("files")).unwrap();
        names
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.ends_with(navo_core::TEMP_SUFFIX))
            .collect()
    }

    /// The job folders under `journal/v1`.
    pub(crate) fn jobs(&self) -> Vec<PathBuf> {
        let Ok(names) = fs::read_dir(self.journal_dir().join("v1")) else {
            return Vec::new();
        };
        names
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.file_name().unwrap() != "owners")
            .collect()
    }
}

pub(crate) fn set_mtime(path: &Path, ms: i64) {
    let time = UNIX_EPOCH + Duration::from_millis(ms.cast_unsigned());
    File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}

pub(crate) fn mtime_ms(path: &Path) -> i64 {
    let time = fs::metadata(path).unwrap().modified().unwrap();
    i64::try_from(time.duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap()
}

/// A fill that writes `NEW`.
pub(crate) fn write_new(out: &mut dyn Write) -> Result<(), String> {
    out.write_all(NEW).map_err(|error| error.to_string())
}

/// Opens a journal on `disk` and overwrites every target with `NEW` in one job; `commit`
/// false aborts it instead. Returns the first error as text.
pub(crate) fn run(disk: Arc<dyn Disk>, fixture: &Fixture, commit: bool) -> Result<(), String> {
    let journal = Journal::open_raw(&fixture.journal_dir(), disk).map_err(|e| e.to_string())?;
    let mut job = journal
        .begin(JobKind::CopyMany, "Copy", None)
        .map_err(|e| e.to_string())?;
    for target in &fixture.targets {
        job.overwrite(target, Expect::default(), None, &mut write_new)
            .map_err(|e| e.to_string())?;
    }
    if !commit {
        let rolled = job.abort("test abort");
        return Err(format!("aborted: {rolled:?}"));
    }
    job.commit().map(|_| ()).map_err(|e| e.to_string())
}

/// The decoded log of the only job folder.
pub(crate) fn only_log(fixture: &Fixture) -> crate::Log {
    let jobs = fixture.jobs();
    assert_eq!(jobs.len(), 1, "one job folder");
    decode(&fs::read(jobs[0].join("log")).unwrap())
}

fn real() -> Arc<dyn Disk> {
    Arc::new(RealDisk)
}

#[test]
fn job_happy_path_writes_new_backs_up_old_and_commits() {
    let fixture = Fixture::new(1);
    run(real(), &fixture, true).unwrap();
    let target = &fixture.targets[0];
    assert_eq!(fs::read(target).unwrap(), NEW);
    assert_eq!(fs::read(fixture.jobs()[0].join("0.bak")).unwrap(), OLD);
    let log = only_log(&fixture);
    let kinds: Vec<&str> = log.records.iter().map(record_name).collect();
    assert_eq!(
        kinds,
        ["Begin", "Intent", "Staged", "Backed", "Done", "Commit"]
    );
    assert!(!log.torn_tail && !log.damaged);
    assert!(fixture.temps().is_empty(), "no temp left");
}

fn record_name(record: &Record) -> &'static str {
    match record {
        Record::Begin { .. } => "Begin",
        Record::Intent { .. } => "Intent",
        Record::Staged { .. } => "Staged",
        Record::Backed { .. } => "Backed",
        Record::Done { .. } => "Done",
        Record::Commit => "Commit",
        Record::Aborted { .. } => "Aborted",
    }
}

#[test]
fn job_change_between_expect_and_backup_gives_changed_with_no_write() {
    let fixture = Fixture::new(1);
    let target = fixture.targets[0].clone();
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let mut job = journal.begin(JobKind::Copy, "Copy", None).unwrap();
    let mut fill = |out: &mut dyn Write| {
        fs::write(&target, b"someone else wrote this").unwrap();
        write_new(out)
    };
    let expect = Expect {
        size: Some(OLD.len() as u64),
        modified_ms: Some(OLD_MS),
        hash: None,
    };
    let result = job.overwrite(&target, expect, None, &mut fill);
    assert_eq!(result, Err(WriteError::Changed));
    assert_eq!(fs::read(&target).unwrap(), b"someone else wrote this");
    assert!(fixture.temps().is_empty(), "no temp left");
    let log = only_log(&fixture);
    assert!(matches!(log.records.last(), Some(Record::Aborted { .. })));
    assert!(
        !log.records
            .iter()
            .any(|r| matches!(r, Record::Backed { .. }))
    );
}

#[test]
fn job_expect_mismatch_is_changed_before_any_intent() {
    let fixture = Fixture::new(1);
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let mut job = journal.begin(JobKind::Copy, "Copy", None).unwrap();
    let expect = Expect {
        size: Some(OLD.len() as u64),
        modified_ms: Some(OLD_MS - 1),
        hash: None,
    };
    let result = job.overwrite(&fixture.targets[0], expect, None, &mut write_new);
    assert_eq!(result, Err(WriteError::Changed));
    assert_eq!(fs::read(&fixture.targets[0]).unwrap(), OLD);
    let log = only_log(&fixture);
    assert!(
        !log.records
            .iter()
            .any(|r| matches!(r, Record::Intent { .. }))
    );
}

#[test]
fn job_read_only_target_gives_read_only_with_no_temp() {
    let fixture = Fixture::new(1);
    let target = &fixture.targets[0];
    fs::set_permissions(target, fs::Permissions::from_mode(0o444)).unwrap();
    let result = run(real(), &fixture, true);
    assert_eq!(result, Err(WriteError::ReadOnly.to_string()));
    assert_eq!(fs::read(target).unwrap(), OLD);
    assert!(fixture.temps().is_empty(), "no temp left");
}

#[test]
fn job_symlink_target_is_refused_and_its_file_untouched() {
    let fixture = Fixture::new(1);
    let link = fixture.dir.path().join("files").join("link.txt");
    std::os::unix::fs::symlink(&fixture.targets[0], &link).unwrap();
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let mut job = journal.begin(JobKind::Copy, "Copy", None).unwrap();
    let result = job.overwrite(&link, Expect::default(), None, &mut write_new);
    assert!(matches!(result, Err(WriteError::Failed(_))), "{result:?}");
    assert_eq!(fs::read(&fixture.targets[0]).unwrap(), OLD);
    assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
}

#[test]
fn job_relative_path_is_refused() {
    let fixture = Fixture::new(1);
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let mut job = journal.begin(JobKind::Copy, "Copy", None).unwrap();
    let result = job.overwrite(Path::new("0.txt"), Expect::default(), None, &mut write_new);
    assert!(matches!(result, Err(WriteError::Failed(_))), "{result:?}");
}

#[test]
fn job_abort_and_drop_restore_old_bytes_and_time() {
    for drop_it in [false, true] {
        let fixture = Fixture::new(2);
        let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
        let mut job = journal.begin(JobKind::CopyMany, "Copy", None).unwrap();
        for target in &fixture.targets {
            job.overwrite(target, Expect::default(), None, &mut write_new)
                .unwrap();
        }
        if drop_it {
            drop(job);
        } else {
            let rolled = job.abort("user cancel");
            assert_eq!(
                (rolled.restored, rolled.failed, rolled.closed),
                (2, 0, true)
            );
        }
        for target in &fixture.targets {
            assert_eq!(fs::read(target).unwrap(), OLD);
            assert_eq!(mtime_ms(target), OLD_MS);
        }
        let reason = if drop_it { "dropped" } else { "user cancel" };
        let last = only_log(&fixture).records.pop();
        assert_eq!(
            last,
            Some(Record::Aborted {
                reason: reason.into()
            })
        );
        assert!(fixture.temps().is_empty());
    }
}

#[test]
fn job_restore_mtime_is_applied_to_the_new_file() {
    let fixture = Fixture::new(1);
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let mut job = journal.begin(JobKind::Undo, "Undo", None).unwrap();
    let target = &fixture.targets[0];
    job.overwrite(
        target,
        Expect::default(),
        Some(OLD_MS + 5_000),
        &mut write_new,
    )
    .unwrap();
    job.commit().unwrap();
    assert_eq!(mtime_ms(target), OLD_MS + 5_000);
}

#[test]
fn job_owner_locks_are_distinct_and_held() {
    let fixture = Fixture::new(0);
    let first = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let second = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    assert_ne!(first.owner(), second.owner());
    let lock = first
        .root()
        .join("owners")
        .join(format!("{}.lock", first.owner()));
    let probe = File::open(&lock).unwrap();
    assert!(probe.try_lock().is_err(), "the owner lock is held");
    drop(first);
    assert!(probe.try_lock().is_ok(), "released with the journal");
}

#[test]
fn job_fault_loop_leaves_old_or_new_and_no_temp() {
    let ops = {
        let fixture = Fixture::new(2);
        let disk = Arc::new(FaultDisk::new(RealDisk, Plan::Pass));
        run(disk.clone(), &fixture, true).unwrap();
        disk.ops()
    };
    assert!(ops > 20, "the loop covers the whole job ({ops} ops)");
    for n in 1..=ops {
        let fixture = Fixture::new(2);
        let disk = Arc::new(FaultDisk::new(RealDisk, Plan::FailAt(n, ErrorKind::Other)));
        let result = run(disk, &fixture, true);
        let logs: Vec<_> = fixture
            .jobs()
            .iter()
            .map(|job| decode(&fs::read(job.join("log")).unwrap()))
            .collect();
        assert!(logs.iter().all(|log| !log.damaged), "op {n}: damaged log");
        // A commit line that was written but not synced is in doubt: the log decides.
        let committed = logs.iter().any(|log| log.records.contains(&Record::Commit));
        assert!(result.is_err() || committed, "op {n}: Ok without a commit");
        for target in &fixture.targets {
            let bytes = fs::read(target).unwrap();
            if committed {
                assert_eq!(bytes, NEW, "op {n}: committed ({result:?})");
            } else {
                assert_eq!(bytes, OLD, "op {n}: rolled back ({result:?})");
                assert_eq!(mtime_ms(target), OLD_MS, "op {n}: old time");
            }
        }
        let temps = fixture.temps();
        assert!(temps.is_empty(), "op {n}: temps {temps:?}");
    }
}
