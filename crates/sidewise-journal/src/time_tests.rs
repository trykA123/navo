//! Tests that a restore time survives the writer's sync and close (V1 BUG-3): exFAT stamps the
//! write time on a file when a handle that wrote to it flushes or closes, over a time set by path.
// Every platform: with TEMP on an exFAT volume the `real_disk` cases test the real driver.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use navo_core::FileStamp;
use sidewise_fs::durable::{Disk, DiskFile, Meta, RealDisk, ReplaceError};

use crate::files::backup_path;
use crate::item::{Expect, Request, stage};
use crate::record::{ItemMode, JobId, Record};
use crate::rollback::roll_back;

/// A restore time on a 2 s boundary, so exFAT's 2 s write-time granularity keeps it exact.
const RESTORE_MS: i64 = 1_700_000_002_000;

/// A file that, like exFAT, stamps the current time on its path at sync and close once written.
struct Stamping {
    inner: Box<dyn DiskFile>,
    path: PathBuf,
    wrote: bool,
}

impl Stamping {
    fn stamp(&self) -> io::Result<()> {
        if self.wrote {
            let file = File::options().write(true).open(&self.path)?;
            file.set_modified(SystemTime::now())?;
        }
        Ok(())
    }
}

impl Write for Stamping {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.wrote |= written > 0;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl DiskFile for Stamping {
    fn sync(&mut self) -> io::Result<()> {
        self.inner.sync()?;
        self.stamp()
    }
}

impl Drop for Stamping {
    #[expect(
        clippy::print_stderr,
        reason = "a failed stamp weakens the test, so it says so"
    )]
    fn drop(&mut self) {
        if let Err(error) = self.stamp() {
            eprintln!("stamp at close failed: {error}");
        }
    }
}

/// `RealDisk` with exFAT's write-time stamping on every file opened for writing.
struct ExfatLike;

impl ExfatLike {
    fn wrap(path: &Path, inner: Box<dyn DiskFile>) -> Box<dyn DiskFile> {
        let path = path.to_path_buf();
        Box::new(Stamping {
            inner,
            path,
            wrote: false,
        })
    }
}

impl Disk for ExfatLike {
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn DiskFile>> {
        Ok(Self::wrap(path, RealDisk.create_new(path)?))
    }
    fn open_append(&self, path: &Path) -> io::Result<Box<dyn DiskFile>> {
        Ok(Self::wrap(path, RealDisk.open_append(path)?))
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

fn mtime_ms(path: &Path) -> i64 {
    let time = fs::metadata(path).unwrap().modified().unwrap();
    i64::try_from(time.duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap()
}

/// Stages new bytes with `RESTORE_MS` and checks both the stamp and the file carry that time.
fn stage_keeps_the_restore_time(disk: &dyn Disk) {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("t.txt");
    fs::write(&target, b"old").unwrap();
    let staged = dir.path().join("t.staged");
    let request = Request {
        target: &target,
        expect: Expect::default(),
        restore_mtime_ms: Some(RESTORE_MS),
    };
    let mut fill = |out: &mut dyn Write| out.write_all(b"new").map_err(|e| e.to_string());
    let stamp = stage(disk, &staged, &request, &mut fill).unwrap();
    assert_eq!(stamp.modified_ms, Some(RESTORE_MS), "the journal stamp");
    assert_eq!(mtime_ms(&staged), RESTORE_MS, "the staged file");
}

/// Rolls back an item whose target is gone and checks the old bytes come back with their time.
fn rollback_keeps_the_old_time(disk: &dyn Disk) {
    let dir = tempfile::tempdir().unwrap();
    let (files, job_dir) = (dir.path().join("files"), dir.path().join("job"));
    fs::create_dir(&files).unwrap();
    fs::create_dir(&job_dir).unwrap();
    let target = files.join("t.txt");
    fs::write(backup_path(&job_dir, 0), b"old").unwrap();
    let old = FileStamp {
        size: 3,
        modified_ms: Some(RESTORE_MS),
        hash: *blake3::hash(b"old").as_bytes(),
    };
    let records = [
        Record::Intent {
            item: 0,
            target: target.to_str().unwrap().to_owned(),
            mode: ItemMode::Overwrite,
            expect_size: Some(3),
            expect_mtime_ms: None,
            restore_mtime_ms: None,
        },
        Record::Backed { item: 0, old },
    ];
    let rolled = roll_back(disk, &job_dir, &JobId::new(1, 2, 3), &records);
    assert_eq!((rolled.restored, rolled.failed), (1, 0));
    assert_eq!(fs::read(&target).unwrap(), b"old");
    assert_eq!(mtime_ms(&target), RESTORE_MS, "the restored target");
}

/// Rolls back an item whose read-only (0444) target holds the new bytes: the restore copy gets
/// the target's mode between its append open and its sync, and keeps the old time.
#[cfg(unix)]
fn rollback_over_a_read_only_target_keeps_mode_and_time(disk: &dyn Disk) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let (files, job_dir) = (dir.path().join("files"), dir.path().join("job"));
    fs::create_dir(&files).unwrap();
    fs::create_dir(&job_dir).unwrap();
    let target = files.join("t.txt");
    fs::write(&target, b"new").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
    fs::write(backup_path(&job_dir, 0), b"old").unwrap();
    let stamp = |bytes: &[u8], ms| FileStamp {
        size: 3,
        modified_ms: ms,
        hash: *blake3::hash(bytes).as_bytes(),
    };
    let records = [
        Record::Intent {
            item: 0,
            target: target.to_str().unwrap().to_owned(),
            mode: ItemMode::Overwrite,
            expect_size: Some(3),
            expect_mtime_ms: None,
            restore_mtime_ms: None,
        },
        Record::Staged {
            item: 0,
            new: stamp(b"new", None),
        },
        Record::Backed {
            item: 0,
            old: stamp(b"old", Some(RESTORE_MS)),
        },
    ];
    let rolled = roll_back(disk, &job_dir, &JobId::new(1, 2, 3), &records);
    assert_eq!((rolled.restored, rolled.failed), (1, 0));
    assert_eq!(fs::read(&target).unwrap(), b"old");
    assert_eq!(mtime_ms(&target), RESTORE_MS, "the restored target");
    let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o444, "the target's mode");
}

#[cfg(unix)]
#[test]
fn rollback_over_a_read_only_target_keeps_mode_and_time_when_close_stamps() {
    rollback_over_a_read_only_target_keeps_mode_and_time(&ExfatLike);
}

#[test]
fn stage_keeps_the_restore_time_on_real_disk() {
    stage_keeps_the_restore_time(&RealDisk);
}

#[test]
fn stage_keeps_the_restore_time_when_close_stamps_the_write_time() {
    stage_keeps_the_restore_time(&ExfatLike);
}

#[test]
fn rollback_keeps_the_old_time_on_real_disk() {
    rollback_keeps_the_old_time(&RealDisk);
}

#[test]
fn rollback_keeps_the_old_time_when_close_stamps_the_write_time() {
    rollback_keeps_the_old_time(&ExfatLike);
}
