//! Crash tests of the job writer (J2): a power loss at every op, with every subset pattern of
//! unsynced changes kept, must leave each target old, or new with a durable verified backup;
//! and a target swapped between backup and replace is caught by identity (NC160).
// Linux only: `RealDisk` is built on Linux until D3.
#![cfg(target_os = "linux")]

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sidewise_fs::durable::{Disk, DiskFile, FaultDisk, Meta, Plan, RealDisk, ReplaceError, Tail};

use crate::job_tests::{Fixture, NEW, OLD, OLD_MS, mtime_ms, run, set_mtime, write_new};
use crate::{Expect, JobKind, Journal, Record, WriteError, decode};

/// Which unsynced changes survive: (name, keep change i, tail of file i).
pub(crate) type Pattern = (&'static str, fn(usize) -> bool, fn(usize) -> Tail);

pub(crate) const PATTERNS: [Pattern; 5] = [
    ("all lost", |_| false, |_| Tail::Lost),
    ("all kept", |_| true, |_| Tail::Kept),
    ("even kept, torn", |i| i % 2 == 0, |_| Tail::Torn),
    (
        "odd kept, mixed",
        |i| i % 2 == 1,
        |f| [Tail::Kept, Tail::Torn, Tail::Lost][f % 3],
    ),
    ("none kept, torn", |_| false, |_| Tail::Torn),
];

/// Checks that a crashed job left a state recovery can roll back: every target is old or new,
/// the log is not damaged, a `Commit` means every target is new, an `Aborted` means every
/// target is old with its old time, a closed job left no temp, and a new target has a
/// `Backed` record whose backup holds the old bytes.
pub(crate) fn assert_recoverable(fixture: &Fixture, case: &str) {
    let log = match fixture.jobs().first() {
        Some(job) => decode(&fs::read(job.join("log")).unwrap_or_default()),
        None => crate::Log::default(),
    };
    assert!(!log.damaged, "{case}: damaged log");
    let committed = log.records.contains(&Record::Commit);
    let aborted = matches!(log.records.last(), Some(Record::Aborted { .. }));
    if committed || aborted {
        assert!(fixture.temps().is_empty(), "{case}: closed job left temps");
    }
    for target in &fixture.targets {
        let bytes = fs::read(target).unwrap_or_else(|e| panic!("{case}: {target:?} {e}"));
        assert!(
            bytes == OLD || bytes == NEW,
            "{case}: torn target {bytes:?}"
        );
        if committed {
            assert_eq!(bytes, NEW, "{case}: committed but old");
        }
        if aborted {
            assert_eq!(
                (bytes.as_slice(), mtime_ms(target)),
                (OLD, OLD_MS),
                "{case}: aborted"
            );
        }
        if bytes == NEW {
            let backup = backup_of(fixture, &log.records, target);
            assert_eq!(
                backup.as_deref(),
                Some(OLD),
                "{case}: new target, no backup"
            );
        }
    }
}

/// The bytes of the backup a `Backed` record names for `target`, if both exist.
fn backup_of(fixture: &Fixture, records: &[Record], target: &Path) -> Option<Vec<u8>> {
    let item = records.iter().find_map(|record| match record {
        Record::Intent {
            item, target: t, ..
        } if Path::new(t) == target => Some(*item),
        _ => None,
    })?;
    let backed = records.iter().find_map(|record| match record {
        Record::Backed { item: i, old } if *i == item => Some(*old),
        _ => None,
    })?;
    let bytes = fs::read(fixture.jobs()[0].join(format!("{item}.bak"))).ok()?;
    let verified = *blake3::hash(&bytes).as_bytes() == backed.hash;
    verified.then_some(bytes)
}

/// The op count of a whole run with no fault.
fn ops_of(commit: bool) -> u64 {
    let fixture = Fixture::new(2);
    let disk = Arc::new(FaultDisk::new(RealDisk, Plan::Pass));
    let _ = run(disk.clone(), &fixture, commit);
    let ops = disk.ops();
    assert!(ops > 40, "the loop covers both items ({ops} ops)");
    ops
}

/// Crashes a two-item job at every op, then keeps each pattern of unsynced changes.
fn crash_loop(commit: bool) {
    let ops = ops_of(commit);
    for n in 1..=ops + 1 {
        for (name, keep, tail) in PATTERNS {
            let fixture = Fixture::new(2);
            let disk = Arc::new(FaultDisk::new(RealDisk, Plan::CrashAt(n)));
            let _ = run(disk.clone(), &fixture, commit);
            disk.crash_with(keep, tail).unwrap();
            assert_recoverable(&fixture, &format!("crash at op {n} of {ops}, {name}"));
        }
    }
}

#[test]
fn job_crash_loop_during_write_and_commit_is_recoverable() {
    crash_loop(true);
}

#[test]
fn job_crash_loop_during_abort_rollback_is_recoverable() {
    crash_loop(false);
}

#[test]
fn job_fail_from_loop_is_recoverable() {
    for n in 1..=ops_of(true) {
        let fixture = Fixture::new(2);
        let plan = Plan::FailFrom(n, io::ErrorKind::Other);
        let _ = run(Arc::new(FaultDisk::new(RealDisk, plan)), &fixture, true);
        assert_recoverable(&fixture, &format!("fail from op {n}"));
    }
}

#[test]
fn job_fault_then_crash_loop_is_recoverable() {
    for n in 1..=ops_of(true) {
        for (name, keep, tail) in PATTERNS {
            let fixture = Fixture::new(2);
            let plan = Plan::FailAt(n, io::ErrorKind::Other);
            let disk = Arc::new(FaultDisk::new(RealDisk, plan));
            let _ = run(disk.clone(), &fixture, true);
            disk.crash_with(keep, tail).unwrap();
            assert_recoverable(&fixture, &format!("fault at op {n}, then crash, {name}"));
        }
    }
}

/// `RealDisk`, except that making a backup first swaps the target for an equal file: same
/// bytes, size and modified time, another identity.
struct SwapOnBackup {
    target: PathBuf,
    swap: PathBuf,
}

impl Disk for SwapOnBackup {
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn DiskFile>> {
        if path.extension().is_some_and(|ext| ext == "bak") {
            fs::rename(&self.swap, &self.target)?;
        }
        RealDisk.create_new(path)
    }
    fn open_append(&self, path: &Path) -> io::Result<Box<dyn DiskFile>> {
        RealDisk.open_append(path)
    }
    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        RealDisk.open_read(path)
    }
    fn meta(&self, path: &Path) -> io::Result<Option<Meta>> {
        RealDisk.meta(path)
    }
    fn set_modified(&self, path: &Path, ms: i64) -> io::Result<()> {
        RealDisk.set_modified(path, ms)
    }
    fn copy_mode(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealDisk.copy_mode(from, to)
    }
    fn replace(&self, staged: &Path, target: &Path, old: &Path) -> Result<(), ReplaceError> {
        RealDisk.replace(staged, target, old)
    }
    fn rename_new(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealDisk.rename_new(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        RealDisk.remove_file(path)
    }
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        RealDisk.create_dir(path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        RealDisk.remove_dir_all(path)
    }
    fn list(&self, dir: &Path) -> io::Result<Vec<String>> {
        RealDisk.list(dir)
    }
    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        RealDisk.sync_dir(dir)
    }
}

#[test]
fn job_target_swapped_before_replace_gives_changed_by_identity() {
    let fixture = Fixture::new(1);
    let target = fixture.targets[0].clone();
    let swap = fixture.dir.path().join("swap.txt");
    fs::write(&swap, OLD).unwrap();
    set_mtime(&swap, OLD_MS);
    let disk = Arc::new(SwapOnBackup {
        target: target.clone(),
        swap,
    });
    let journal = Journal::open_raw(&fixture.journal_dir(), disk).unwrap();
    let mut job = journal.begin(JobKind::Copy, "Copy", None).unwrap();
    let result = job.overwrite(&target, Expect::default(), None, &mut write_new);
    assert_eq!(result, Err(WriteError::Changed));
    assert_eq!(
        fs::read(&target).unwrap(),
        OLD,
        "the swapped-in file is kept"
    );
    assert!(fixture.temps().is_empty());
}
