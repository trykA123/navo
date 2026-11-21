//! Tests of the retention sweep (J4, NC141, NC165): age and size limits, the jobs it must keep
//! (open, live owner, damaged, young empty), leftover trash, and a crash at every sweep op.
// Linux only: `RealDisk` is built on Linux until D3.
#![cfg(target_os = "linux")]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sidewise_fs::durable::{FaultDisk, Plan, RealDisk};

use super::{CAP_BYTES, Swept};
use crate::job_crash_tests::PATTERNS;
use crate::journal::now_ms;
use crate::{JobId, JobKind, Journal, Record, encode};

const DAY: u64 = 86_400_000;
const MIB: u64 = 1 << 20;
/// The sweep time of the tests that call `sweep` themselves.
const NOW: u64 = 1_800_000_000_000;
/// An owner stem with no lock file: a dead process.
const DEAD: &str = "999999-1";

/// How a fake job's log ends.
#[derive(Clone, Copy)]
enum End {
    Commit,
    Open,
    Damaged,
}

/// A journal opened with no recovery in a temp folder.
fn journal() -> (tempfile::TempDir, Journal) {
    let dir = tempfile::tempdir().unwrap();
    let journal = Journal::open_raw(&dir.path().join("journal"), Arc::new(RealDisk)).unwrap();
    (dir, journal)
}

/// Writes a job folder `<id>/log` made at `created_ms` by `owner`, ending as `end`, with a
/// sparse `0.bak` of `bak` bytes. Returns the folder.
fn fake_job(root: &Path, created_ms: u64, (owner, end): (&str, End), bak: u64) -> PathBuf {
    let id = JobId::new(created_ms, 4242, created_ms);
    let dir = root.join(id.as_str());
    fs::create_dir(&dir).unwrap();
    let begin = Record::Begin {
        job: id,
        kind: JobKind::Copy,
        label: "Copy".to_owned(),
        created_ms,
        reverts: None,
        owner: owner.to_owned(),
    };
    let mut log = encode(&begin);
    match end {
        End::Commit => log.push_str(&encode(&Record::Commit)),
        End::Open => {}
        End::Damaged => log.push_str("not a record\n"),
    }
    fs::write(dir.join("log"), log).unwrap();
    fs::File::create(dir.join("0.bak"))
        .unwrap()
        .set_len(bak)
        .unwrap();
    dir
}

/// An empty job folder (a crash inside `begin`) made `age` ms before `now`.
fn empty_job(root: &Path, now: u64, age: u64) -> PathBuf {
    let dir = root.join(JobId::new(now - age, 4343, age).as_str());
    fs::create_dir(&dir).unwrap();
    dir
}

/// The logical bytes of the files in `dir`.
fn bytes(dir: &Path) -> u64 {
    let entries = fs::read_dir(dir).unwrap();
    entries
        .map(|entry| entry.unwrap().metadata().unwrap().len())
        .sum::<u64>()
}

/// The names in the journal's `v1` folder.
fn names(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn sweep_removes_only_the_job_older_than_14_days() {
    let (_dir, journal) = journal();
    let root = journal.root().to_path_buf();
    let old = fake_job(&root, NOW - 15 * DAY, (DEAD, End::Commit), 10);
    let mid = fake_job(&root, NOW - 13 * DAY, (DEAD, End::Commit), 10);
    let new = fake_job(&root, NOW - DAY, (DEAD, End::Commit), 10);
    let want = bytes(&mid) + bytes(&new);
    let swept = journal.sweep(NOW).unwrap();
    let want = Swept {
        removed: 1,
        bytes_left: want,
        failed: 0,
    };
    assert_eq!(swept, want);
    assert!(!old.exists() && mid.exists() && new.exists());
    assert!(names(&root).iter().all(|name| !name.ends_with(".trash")));
}

#[test]
fn sweep_removes_the_oldest_job_while_over_the_cap() {
    let (_dir, journal) = journal();
    let root = journal.root().to_path_buf();
    let jobs: Vec<PathBuf> = [3, 2, 1]
        .map(|days| fake_job(&root, NOW - days * DAY, (DEAD, End::Commit), 400 * MIB))
        .into();
    let swept = journal.sweep(NOW).unwrap();
    assert_eq!(swept.removed, 1);
    assert_eq!(swept.bytes_left, bytes(&jobs[1]) + bytes(&jobs[2]));
    assert!(swept.bytes_left <= CAP_BYTES);
    assert!(!jobs[0].exists() && jobs[1].exists() && jobs[2].exists());
}

#[test]
fn sweep_keeps_open_live_own_and_damaged_jobs_over_age_and_cap() {
    let (dir, journal) = journal();
    let live = Journal::open_raw(&dir.path().join("journal"), Arc::new(RealDisk)).unwrap();
    let root = journal.root().to_path_buf();
    let kept = [
        fake_job(&root, NOW - 20 * DAY, (DEAD, End::Open), 600 * MIB),
        fake_job(
            &root,
            NOW - 21 * DAY,
            (live.owner(), End::Commit),
            600 * MIB,
        ),
        fake_job(&root, NOW - 22 * DAY, (journal.owner(), End::Commit), 10),
        fake_job(&root, NOW - 23 * DAY, (DEAD, End::Damaged), 10),
    ];
    let swept = journal.sweep(NOW).unwrap();
    assert_eq!(swept.removed, 0);
    assert_eq!(
        swept.bytes_left,
        kept.iter().map(|job| bytes(job)).sum::<u64>()
    );
    assert!(swept.bytes_left > CAP_BYTES);
    assert!(kept.iter().all(|job| job.join("log").exists()));
}

#[test]
fn sweep_removes_an_empty_job_by_age_only_never_by_the_cap() {
    let (dir, journal) = journal();
    let live = Journal::open_raw(&dir.path().join("journal"), Arc::new(RealDisk)).unwrap();
    let root = journal.root().to_path_buf();
    let big = fake_job(
        &root,
        NOW - 2 * DAY,
        (live.owner(), End::Commit),
        1200 * MIB,
    );
    let old_empty = empty_job(&root, NOW, 15 * DAY);
    let young_empty = empty_job(&root, NOW, 3 * DAY);
    let swept = journal.sweep(NOW).unwrap();
    assert_eq!(swept.removed, 1);
    assert!(!old_empty.exists() && young_empty.exists() && big.exists());
}

#[test]
fn sweep_removes_leftover_trash_and_keeps_foreign_names() {
    let (_dir, journal) = journal();
    let root = journal.root().to_path_buf();
    fs::create_dir_all(root.join("x.trash/inner")).unwrap();
    fs::write(root.join("x.trash/inner/0.bak"), b"old").unwrap();
    fs::create_dir(root.join("notes")).unwrap();
    journal.sweep(NOW).unwrap();
    assert_eq!(names(&root), ["notes", "owners"]);
}

#[test]
fn open_finishes_a_removal_that_crashed_between_rename_and_delete() {
    let (dir, journal) = journal();
    let root = journal.root().to_path_buf();
    let old = fake_job(&root, now() - 15 * DAY, (DEAD, End::Commit), 10);
    let mut trash = old.clone().into_os_string();
    trash.push(".trash");
    fs::rename(&old, &trash).unwrap();
    drop(journal);
    let (journal, _) = Journal::open(&dir.path().join("journal"), Arc::new(RealDisk)).unwrap();
    assert_eq!(names(journal.root()), ["owners"]);
}

/// The wall clock as the sweep in `open` sees it.
fn now() -> u64 {
    now_ms().cast_unsigned()
}

/// A journal folder with an old and a young committed job of a dead owner; returns the
/// folder, the old job and the young job with its log bytes.
fn crash_fixture() -> (tempfile::TempDir, PathBuf, PathBuf, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let journal_dir = dir.path().join("journal");
    let root = Journal::open_raw(&journal_dir, Arc::new(RealDisk))
        .unwrap()
        .root()
        .to_path_buf();
    let old = fake_job(&root, now() - 15 * DAY, (DEAD, End::Commit), 10);
    let young = fake_job(&root, now() - DAY, (DEAD, End::Commit), 10);
    let log = fs::read(young.join("log")).unwrap();
    (dir, old, young, log)
}

/// Opens the journal on `disk` with no recovery and sweeps it; returns the op count.
fn sweep_on(journal_dir: &Path, disk: &Arc<FaultDisk>) -> u64 {
    if let Ok(journal) = Journal::open_raw(journal_dir, disk.clone()) {
        let _ = journal.sweep(now());
    }
    disk.ops()
}

#[test]
fn open_settles_a_sweep_that_crashed_at_every_op() {
    let (dir, ..) = crash_fixture();
    let ops = sweep_on(
        &dir.path().join("journal"),
        &Arc::new(FaultDisk::new(RealDisk, Plan::Pass)),
    );
    assert!(ops > 8, "the sweep renames and deletes ({ops} ops)");
    for n in 1..=ops + 1 {
        for (name, keep, tail) in PATTERNS {
            let (dir, old, young, log) = crash_fixture();
            let journal_dir = dir.path().join("journal");
            let disk = Arc::new(FaultDisk::new(RealDisk, Plan::CrashAt(n)));
            sweep_on(&journal_dir, &disk);
            disk.crash_with(keep, tail).unwrap();
            let (journal, _) = Journal::open(&journal_dir, Arc::new(RealDisk)).unwrap();
            let case = format!("crash at op {n} of {ops}, {name}");
            assert!(!old.exists(), "{case}: old job left");
            assert_eq!(fs::read(young.join("log")).unwrap(), log, "{case}");
            assert_eq!(bytes(&young), log.len() as u64 + 10, "{case}");
            let left = names(journal.root());
            assert!(left.iter().all(|name| !name.ends_with(".trash")), "{case}");
        }
    }
}

#[test]
fn open_reports_what_the_sweep_did() {
    let (dir, _old, young, _log) = crash_fixture();
    let (_journal, recovered) =
        Journal::open(&dir.path().join("journal"), Arc::new(RealDisk)).unwrap();
    let want = Swept {
        removed: 1,
        bytes_left: bytes(&young),
        failed: 0,
    };
    assert_eq!(recovered.swept, want);
}

#[test]
fn sweep_removes_the_old_job_of_a_dead_owner_whose_lock_file_is_left() {
    let (_dir, journal) = journal();
    let root = journal.root().to_path_buf();
    let lock = root.join("owners").join(format!("{DEAD}.lock"));
    fs::write(&lock, b"").unwrap();
    let old = fake_job(&root, NOW - 15 * DAY, (DEAD, End::Commit), 10);
    let swept = journal.sweep(NOW).unwrap();
    assert_eq!((swept.removed, swept.failed), (1, 0));
    assert!(!old.exists());
    assert!(lock.exists(), "the sweep never deletes a lock file");
}

#[test]
fn sweep_never_reads_or_removes_a_job_folder_that_is_a_link() {
    let (dir, journal) = journal();
    let root = journal.root().to_path_buf();
    let job = fake_job(&root, NOW - 15 * DAY, (DEAD, End::Commit), 10);
    let outside = dir.path().join("outside");
    fs::rename(&job, &outside).unwrap();
    std::os::unix::fs::symlink(&outside, &job).unwrap();
    let swept = journal.sweep(NOW).unwrap();
    let want = Swept {
        removed: 0,
        bytes_left: 0,
        failed: 1,
    };
    assert_eq!(swept, want);
    assert!(
        fs::symlink_metadata(&job).unwrap().is_symlink(),
        "link kept"
    );
    assert_eq!(names(&outside), ["0.bak", "log"]);
}

#[test]
fn sweep_removes_a_trash_link_but_not_the_folder_it_points_at() {
    let (dir, journal) = journal();
    let root = journal.root().to_path_buf();
    let outside = dir.path().join("outside");
    fs::create_dir_all(outside.join("inner")).unwrap();
    fs::write(outside.join("inner/keep.txt"), b"user data").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("x.trash")).unwrap();
    let swept = journal.sweep(NOW).unwrap();
    assert_eq!(swept.failed, 0);
    assert_eq!(names(&root), ["owners"]);
    let kept = fs::read(outside.join("inner/keep.txt")).unwrap();
    assert_eq!(kept, b"user data");
}

#[test]
fn sweep_counts_the_bytes_of_a_removal_whose_delete_failed() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, journal) = journal();
    let root = journal.root().to_path_buf();
    let old = fake_job(&root, NOW - 15 * DAY, (DEAD, End::Commit), 10);
    let want = bytes(&old);
    // No write permission inside: the rename works, deleting its files does not.
    fs::set_permissions(&old, fs::Permissions::from_mode(0o555)).unwrap();
    let swept = journal.sweep(NOW).unwrap();
    let trash = root.join(format!(
        "{}.trash",
        old.file_name().unwrap().to_str().unwrap()
    ));
    fs::set_permissions(&trash, fs::Permissions::from_mode(0o755)).unwrap();
    let want = Swept {
        removed: 1,
        bytes_left: want,
        failed: 1,
    };
    assert_eq!(swept, want);
}
