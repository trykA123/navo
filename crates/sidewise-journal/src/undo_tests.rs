//! Tests of journal undo (J5): a copy undone byte for byte with its old time, refusals that
//! write nothing, a backup that fails its hash, and a target edited between check and backup.
// Linux only: `RealDisk` is built on Linux until D3.
#![cfg(target_os = "linux")]

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use navo_core::FileStamp;
use sidewise_fs::durable::{Disk, DiskFile, Meta, RealDisk, ReplaceError};

use super::UndoError;
use crate::job_tests::{Fixture, NEW, OLD, OLD_MS, mtime_ms, set_mtime, write_new};
use crate::{Expect, ItemMode, JobId, JobKind, Journal, Record, decode, encode};

/// Copies `NEW` over every target of `fixture` in one committed `CopyMany` job.
pub(crate) fn copied(journal: &Journal, fixture: &Fixture) -> Result<JobId, String> {
    let mut job = journal
        .begin(JobKind::CopyMany, "Copy", None)
        .map_err(|e| e.to_string())?;
    for target in &fixture.targets {
        job.overwrite(target, Expect::default(), None, &mut write_new)
            .map_err(|e| e.to_string())?;
    }
    job.commit().map_err(|e| e.to_string())
}

fn real() -> Arc<dyn Disk> {
    Arc::new(RealDisk)
}

/// The decoded logs of every job folder whose `Begin` is an `Undo`.
fn undo_logs(fixture: &Fixture) -> Vec<crate::Log> {
    let logs = fixture
        .jobs()
        .into_iter()
        .map(|job| decode(&fs::read(job.join("log")).unwrap_or_default()));
    logs.filter(|log| {
        matches!(
            log.records.first(),
            Some(Record::Begin {
                kind: JobKind::Undo,
                ..
            })
        )
    })
    .collect()
}

/// Asserts every target still holds `NEW` with the time it got from the copy.
fn assert_still_new(fixture: &Fixture, times: &[i64]) {
    for (target, ms) in fixture.targets.iter().zip(times) {
        assert_eq!(fs::read(target).unwrap(), NEW, "{target:?} kept the copy");
        assert_eq!(mtime_ms(target), *ms, "{target:?} kept its time");
    }
}

fn times(fixture: &Fixture) -> Vec<i64> {
    fixture.targets.iter().map(|t| mtime_ms(t)).collect()
}

#[test]
fn undo_restores_the_original_bytes_and_mtime() {
    let fixture = Fixture::new(2);
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let id = copied(&journal, &fixture).unwrap();
    let undone = journal.undo(&id).unwrap();
    assert_eq!(undone, fixture.targets);
    for target in &fixture.targets {
        assert_eq!(fs::read(target).unwrap(), OLD);
        assert_eq!(mtime_ms(target), OLD_MS, "the old modified time is back");
    }
    let logs = undo_logs(&fixture);
    assert_eq!(logs.len(), 1, "one undo job");
    let records = &logs[0].records;
    assert!(matches!(
        records.first(),
        Some(Record::Begin { reverts: Some(reverted), .. }) if *reverted == id
    ));
    assert_eq!(records.last(), Some(&Record::Commit));
    let order: Vec<u32> = records
        .iter()
        .filter_map(|record| match record {
            Record::Intent { target, .. } => fixture
                .targets
                .iter()
                .position(|t| Path::new(target) == t)
                .map(|i| u32::try_from(i).unwrap()),
            _ => None,
        })
        .collect();
    assert_eq!(order, [1, 0], "items run newest first");
    assert!(fixture.temps().is_empty(), "no temp left");
}

#[test]
fn undo_of_a_target_edited_after_the_copy_is_changed_with_no_write() {
    let fixture = Fixture::new(2);
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let id = copied(&journal, &fixture).unwrap();
    fs::write(&fixture.targets[1], b"edited by the user\n").unwrap();
    let first_ms = mtime_ms(&fixture.targets[0]);
    let result = journal.undo(&id);
    assert_eq!(result, Err(UndoError::Changed(fixture.targets[1].clone())));
    assert_eq!(fs::read(&fixture.targets[0]).unwrap(), NEW);
    assert_eq!(mtime_ms(&fixture.targets[0]), first_ms);
    assert_eq!(
        fs::read(&fixture.targets[1]).unwrap(),
        b"edited by the user\n"
    );
    assert_eq!(fixture.jobs().len(), 1, "no undo job was begun");
}

#[test]
fn undo_of_a_target_with_only_its_time_changed_is_changed() {
    let fixture = Fixture::new(1);
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let id = copied(&journal, &fixture).unwrap();
    set_mtime(&fixture.targets[0], OLD_MS + 5);
    let result = journal.undo(&id);
    assert_eq!(result, Err(UndoError::Changed(fixture.targets[0].clone())));
    assert_eq!(fs::read(&fixture.targets[0]).unwrap(), NEW);
}

#[test]
fn undo_twice_gives_changed_the_second_time() {
    let fixture = Fixture::new(1);
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let id = copied(&journal, &fixture).unwrap();
    journal.undo(&id).unwrap();
    let again = journal.undo(&id);
    assert_eq!(again, Err(UndoError::Changed(fixture.targets[0].clone())));
    assert_eq!(fs::read(&fixture.targets[0]).unwrap(), OLD);
}

#[test]
fn undo_of_an_aborted_job_is_not_undoable() {
    let fixture = Fixture::new(1);
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let mut job = journal.begin(JobKind::Copy, "Copy", None).unwrap();
    let id = job.id().clone();
    job.overwrite(&fixture.targets[0], Expect::default(), None, &mut write_new)
        .unwrap();
    job.abort("test");
    assert!(matches!(journal.undo(&id), Err(UndoError::NotUndoable(_))));
    assert_eq!(fixture.jobs().len(), 1, "no undo job was begun");
}

#[test]
fn undo_of_an_undo_job_is_not_undoable() {
    let fixture = Fixture::new(1);
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let id = copied(&journal, &fixture).unwrap();
    journal.undo(&id).unwrap();
    let undo_dir = fixture
        .jobs()
        .into_iter()
        .find(|job| job.file_name().unwrap() != id.as_str())
        .unwrap();
    let undo_id = JobId::try_from(undo_dir.file_name().unwrap().to_str().unwrap().to_owned());
    let result = journal.undo(&undo_id.unwrap());
    assert!(
        matches!(result, Err(UndoError::NotUndoable(_))),
        "{result:?}"
    );
    assert_eq!(fs::read(&fixture.targets[0]).unwrap(), OLD);
}

#[test]
fn undo_of_another_sessions_job_is_not_undoable() {
    let fixture = Fixture::new(1);
    let other = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let id = copied(&other, &fixture).unwrap();
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    assert_ne!(journal.owner(), other.owner());
    assert!(matches!(journal.undo(&id), Err(UndoError::NotUndoable(_))));
    assert_eq!(fs::read(&fixture.targets[0]).unwrap(), NEW);
}

#[test]
fn undo_of_a_missing_job_is_not_found() {
    let fixture = Fixture::new(1);
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let id = JobId::new(1_700_000_000_000, 1, 1);
    assert_eq!(journal.undo(&id), Err(UndoError::NotFound));
}

#[test]
fn undo_of_a_damaged_log_writes_nothing() {
    let fixture = Fixture::new(1);
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let id = copied(&journal, &fixture).unwrap();
    let log = fixture.jobs()[0].join("log");
    let mut bytes = fs::read(&log).unwrap();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0x01;
    fs::write(&log, bytes).unwrap();
    assert!(matches!(journal.undo(&id), Err(UndoError::Damaged(_))));
    assert_eq!(fs::read(&fixture.targets[0]).unwrap(), NEW);
}

#[test]
fn undo_of_a_job_with_a_create_item_is_unsupported_with_no_write() {
    let fixture = Fixture::new(1);
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let id = JobId::new(1_700_000_000_000, std::process::id(), 99);
    let dir = journal.root().join(id.as_str());
    fs::create_dir(&dir).unwrap();
    let stamp = FileStamp {
        size: 1,
        modified_ms: Some(OLD_MS),
        hash: [0; 32],
    };
    let records = [
        Record::Begin {
            job: id.clone(),
            kind: JobKind::Sync,
            label: "Sync".into(),
            created_ms: 1_700_000_000_000,
            reverts: None,
            owner: journal.owner().to_owned(),
        },
        Record::Intent {
            item: 0,
            target: fixture.targets[0].to_str().unwrap().to_owned(),
            mode: ItemMode::Create,
            expect_size: None,
            expect_mtime_ms: None,
            restore_mtime_ms: None,
        },
        Record::Staged {
            item: 0,
            new: stamp,
        },
        Record::Done { item: 0 },
        Record::Commit,
    ];
    let text: String = records.iter().map(encode).collect();
    fs::write(dir.join("log"), text).unwrap();
    assert_eq!(journal.undo(&id), Err(UndoError::Unsupported));
    assert_eq!(fs::read(&fixture.targets[0]).unwrap(), OLD);
    assert_eq!(fixture.jobs().len(), 1, "no undo job was begun");
}

#[test]
fn undo_with_a_backup_that_fails_its_hash_rolls_back_and_keeps_new() {
    let fixture = Fixture::new(2);
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let id = copied(&journal, &fixture).unwrap();
    let before = times(&fixture);
    // Same length, one byte off: only the hash can tell.
    fs::write(fixture.jobs()[0].join("0.bak"), b"old bytez\n").unwrap();
    let result = journal.undo(&id);
    assert!(matches!(result, Err(UndoError::Write(_))), "{result:?}");
    assert_still_new(&fixture, &before);
    assert!(fixture.temps().is_empty(), "no temp left");
    let logs = undo_logs(&fixture);
    assert!(matches!(
        logs[0].records.last(),
        Some(Record::Aborted { .. })
    ));
}

#[test]
fn undo_of_a_read_only_target_writes_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new(1);
    let journal = Journal::open_raw(&fixture.journal_dir(), real()).unwrap();
    let id = copied(&journal, &fixture).unwrap();
    let target = &fixture.targets[0];
    fs::set_permissions(target, fs::Permissions::from_mode(0o444)).unwrap();
    let result = journal.undo(&id);
    assert_eq!(result, Err(UndoError::Write(crate::WriteError::ReadOnly)));
    assert_eq!(fs::read(target).unwrap(), NEW);
}

/// `RealDisk`, except that once `armed`, making a file whose name ends in `at` first rewrites
/// `target` in place with `x` bytes: `extra` more than it had, with its modified time put back.
/// With no extra byte, only a hash can tell.
struct EditOn {
    target: PathBuf,
    armed: AtomicBool,
    at: &'static str,
    extra: u64,
}

impl EditOn {
    fn new(target: PathBuf, at: &'static str, extra: u64) -> Arc<Self> {
        let armed = AtomicBool::new(false);
        Arc::new(Self {
            target,
            armed,
            at,
            extra,
        })
    }
}

impl Disk for EditOn {
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn DiskFile>> {
        let hit = path.to_string_lossy().ends_with(self.at);
        if hit && self.armed.swap(false, Ordering::SeqCst) {
            let ms = mtime_ms(&self.target);
            let len = fs::metadata(&self.target)?.len() + self.extra;
            let mut file = OpenOptions::new().write(true).open(&self.target)?;
            file.write_all(&vec![b'x'; usize::try_from(len).unwrap()])?;
            drop(file);
            set_mtime(&self.target, ms);
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
fn undo_of_a_target_edited_between_check_and_backup_keeps_the_edit() {
    let fixture = Fixture::new(1);
    let target = fixture.targets[0].clone();
    let disk = EditOn::new(target.clone(), ".bak", 0);
    let journal = Journal::open_raw(&fixture.journal_dir(), disk.clone()).unwrap();
    let id = copied(&journal, &fixture).unwrap();
    disk.armed.store(true, Ordering::SeqCst);
    let result = journal.undo(&id);
    assert_eq!(result, Err(UndoError::Changed(target.clone())));
    assert_eq!(
        fs::read(&target).unwrap(),
        vec![b'x'; NEW.len()],
        "the edit made during the undo is kept"
    );
    assert!(fixture.temps().is_empty(), "no temp left");
}

#[test]
fn undo_of_a_target_grown_after_the_check_is_changed_and_keeps_the_edit() {
    let fixture = Fixture::new(1);
    let target = fixture.targets[0].clone();
    // The undo job's log is made after the check and before the write's own check.
    let disk = EditOn::new(target.clone(), "/log", 1);
    let journal = Journal::open_raw(&fixture.journal_dir(), disk.clone()).unwrap();
    let id = copied(&journal, &fixture).unwrap();
    disk.armed.store(true, Ordering::SeqCst);
    let result = journal.undo(&id);
    assert_eq!(result, Err(UndoError::Changed(target.clone())));
    assert_eq!(fs::read(&target).unwrap(), vec![b'x'; NEW.len() + 1]);
    let logs = undo_logs(&fixture);
    assert!(matches!(
        logs[0].records.last(),
        Some(Record::Aborted { .. })
    ));
}
