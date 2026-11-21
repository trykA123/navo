//! Races of recovery at open (J3): a job that commits between the listing and the claim, and a
//! user save into the target while the rollback copies the backup (NC160 before the replace).
// Linux only: `RealDisk` is built on Linux until D3.
#![cfg(target_os = "linux")]

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sidewise_fs::durable::{Disk, DiskFile, Meta, RealDisk, ReplaceError};

use crate::job_tests::{Fixture, NEW, OLD, only_log, write_new};
use crate::recover_crash_tests::check_settled;
use crate::recover_tests::crashed_open_job;
use crate::{Expect, JobKind, Journal, Record, Recovered};

/// What the user saves into the target during the rollback; its length differs from `NEW`.
const SAVED: &[u8] = b"saved by the user\n";

#[test]
fn recover_rereads_a_log_that_committed_after_the_listing() {
    let fixture = Fixture::new(1);
    let live = Journal::open_raw(&fixture.journal_dir(), Arc::new(RealDisk)).unwrap();
    let mut job = live.begin(JobKind::Copy, "Copy", None).unwrap();
    job.overwrite(&fixture.targets[0], Expect::default(), None, &mut write_new)
        .unwrap();
    let commit_and_exit = move || {
        job.commit().unwrap();
        drop(live);
    };
    let (_journal, recovered) =
        Journal::open_with(&fixture.journal_dir(), Arc::new(RealDisk), commit_and_exit).unwrap();
    assert_eq!(recovered, Recovered::default());
    assert_eq!(only_log(&fixture).records.last(), Some(&Record::Commit));
    assert_eq!(fs::read(&fixture.targets[0]).unwrap(), NEW);
    check_settled(&fixture, "commit after the listing").unwrap();
}

/// `RealDisk`, but when the rollback makes its restore temp (`…-r.navo-part`), the user saves
/// `SAVED` into `target` in place.
struct SaveDuringRestore {
    target: PathBuf,
}

impl Disk for SaveDuringRestore {
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn DiskFile>> {
        if path.to_string_lossy().ends_with("-r.navo-part") {
            fs::write(&self.target, SAVED)?;
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
fn recover_keeps_a_user_save_made_during_the_restore_copy() {
    let fixture = Fixture::new(1);
    crashed_open_job(&fixture, 1);
    let disk = Arc::new(SaveDuringRestore {
        target: fixture.targets[0].clone(),
    });
    let (_journal, recovered) = Journal::open(&fixture.journal_dir(), disk).unwrap();
    let want = Recovered {
        kept: 1,
        rolled_back: 1,
        swept: recovered.swept,
        ..Recovered::default()
    };
    assert_eq!(recovered, want);
    assert_eq!(
        fs::read(&fixture.targets[0]).unwrap(),
        SAVED,
        "user save kept"
    );
    assert_eq!(fs::read(fixture.jobs()[0].join("0.bak")).unwrap(), OLD);
    assert_eq!(
        fixture.temps(),
        Vec::<String>::new(),
        "no restore temp left"
    );
}
