//! `Job`: one journaled write job (NC143, NC144). `overwrite` writes one item in the NC144
//! order; `commit` appends the commit point; any error, `abort` or a drop rolls it back (NC145).

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sidewise_fs::durable::DiskFile;

use crate::error::{JournalError, WriteError, classify, journal_io};
use crate::files::{Temp, backup_path, temp_path};
use crate::item::{Expect, Fill, Request, back_up, precheck, replace_error, stage, still_same};
use crate::journal::Inner;
use crate::record::{ItemMode, JobId, Record, encode};
use crate::rollback::{Rolled, roll_back};

#[cfg(test)]
thread_local! {
    /// Test-only mutation (J3): when set, the `Backed` line is written but not synced.
    pub(crate) static SKIP_BACKED_SYNC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// One open job. Dropping it without `commit` or `abort` aborts it.
pub struct Job {
    inner: Arc<Inner>,
    id: JobId,
    dir: PathBuf,
    log: Box<dyn DiskFile>,
    /// False once an append failed: the log may end in a torn line, so it is never written
    /// again and the job is left for recovery (J3) to close.
    log_ok: bool,
    /// True when the last append wrote its whole line but its sync failed.
    in_doubt: bool,
    /// Every record durably appended, in order; the rollback reads it.
    records: Vec<Record>,
    next_item: u32,
    open: bool,
}

impl Job {
    /// Makes the job folder (synced into `v1`), then the log with its synced `Begin` line and
    /// the synced log entry. On failure it tries to remove the half-made folder.
    pub(crate) fn start(
        inner: Arc<Inner>,
        id: JobId,
        begin: &Record,
    ) -> Result<Self, JournalError> {
        let disk = Arc::clone(&inner.disk);
        let dir = inner.root.join(id.as_str());
        disk.create_dir(&dir)
            .map_err(|error| journal_io("make job folder", &error))?;
        let made = (|| {
            disk.sync_dir(&inner.root)?;
            let mut log = disk.create_new(&dir.join("log"))?;
            log.write_all(encode(begin).as_bytes())?;
            log.sync()?;
            disk.sync_dir(&dir)?;
            Ok::<_, io::Error>(log)
        })();
        let log = match made {
            Ok(log) => log,
            Err(error) => {
                // Best effort: a folder with no durable `Begin` holds no write; J3 removes it too.
                let _ = disk
                    .remove_dir_all(&dir)
                    .and_then(|()| disk.sync_dir(&inner.root));
                return Err(journal_io("start job log", &error));
            }
        };
        Ok(Self {
            inner,
            id,
            dir,
            log,
            log_ok: true,
            in_doubt: false,
            records: vec![begin.clone()],
            next_item: 0,
            open: true,
        })
    }

    /// The job's id, also its folder name.
    pub fn id(&self) -> &JobId {
        &self.id
    }

    /// Writes `fill`'s bytes over the existing file `target` (absolute) in the NC144 order,
    /// with a durable backup of the old bytes first. `restore_mtime_ms` sets the new file's
    /// modified time; `None` keeps the write time. Returns the item number. On any error the
    /// whole job is rolled back and closed, and the error says why (NC147); `NotRolledBack`
    /// when some items could not be rolled back now, so those targets may hold new bytes.
    #[expect(
        clippy::too_many_arguments,
        reason = "the J2 signature of the P1 plan: target, expect, restore time and fill"
    )]
    pub fn overwrite(
        &mut self,
        target: &Path,
        expect: Expect,
        restore_mtime_ms: Option<i64>,
        fill: Fill<'_>,
    ) -> Result<u32, WriteError> {
        if !self.open {
            return Err(WriteError::Failed("the job is closed".into()));
        }
        let request = Request {
            target,
            expect,
            restore_mtime_ms,
        };
        self.write_item(&request, fill).map_err(|cause| {
            let rolled = self.close(&cause.to_string());
            if rolled.failed == 0 {
                return cause;
            }
            WriteError::NotRolledBack {
                cause: Box::new(cause),
                failed: rolled.failed,
            }
        })
    }

    /// The NC144 steps 1 to 9 for one item; see `overwrite`.
    fn write_item(&mut self, request: &Request<'_>, fill: Fill<'_>) -> Result<u32, WriteError> {
        let disk = Arc::clone(&self.inner.disk);
        let target = request.target;
        let before = precheck(disk.as_ref(), request)?;
        let item = self.next_item;
        self.next_item += 1;
        self.append(Record::Intent {
            item,
            target: target.to_str().unwrap_or_default().to_owned(),
            mode: ItemMode::Overwrite,
            expect_size: Some(before.size),
            expect_mtime_ms: before.modified_ms,
            restore_mtime_ms: request.restore_mtime_ms,
        })?;
        let temp =
            |kind| temp_path(target, &self.id, item, kind).map_err(|e| classify("name temp", &e));
        let (staged, old_temp) = (temp(Temp::Staged)?, temp(Temp::Old)?);
        let new = stage(disk.as_ref(), &staged, request, fill)?;
        self.append(Record::Staged { item, new })?;
        let backup = backup_path(&self.dir, item);
        let old = back_up(disk.as_ref(), request, &backup, &before)?;
        disk.sync_dir(&self.dir)
            .map_err(|error| classify("sync job folder", &error))?;
        self.append(Record::Backed { item, old })?;
        let same = still_same(disk.as_ref(), target, &before, &old.hash)
            .map_err(|e| classify("re-check target", &e))?;
        if !same {
            return Err(WriteError::Changed);
        }
        disk.replace(&staged, target, &old_temp)
            .map_err(replace_error)?;
        self.append(Record::Done { item })?;
        Ok(item)
    }

    /// Appends the commit point. When the line is not written, the job is rolled back. When
    /// it is written but its sync fails, the commit is in doubt (NC162): the job is left as it
    /// is, with the new bytes, for recovery at the next start, and the error is `InDoubt`.
    pub fn commit(mut self) -> Result<JobId, JournalError> {
        if !self.open {
            return Err(JournalError::Io("the job was rolled back".into()));
        }
        match self.append(Record::Commit) {
            Ok(()) => {
                self.open = false;
                Ok(self.id.clone())
            }
            Err(error) if self.in_doubt => {
                self.open = false;
                Err(JournalError::InDoubt(error.to_string()))
            }
            Err(error) => {
                // Files left to roll back keep the new bytes: recovery settles them (NC168).
                let rolled = self.close("commit failed");
                if rolled.failed == 0 {
                    return Err(JournalError::Io(error.to_string()));
                }
                Err(JournalError::InDoubt(format!(
                    "{error}; {} file(s) not rolled back yet",
                    rolled.failed
                )))
            }
        }
    }

    /// Rolls the job back (NC145) and appends `Aborted {reason}` when every item ended clean.
    pub fn abort(mut self, reason: &str) -> Rolled {
        self.close(reason)
    }

    /// Rolls back once and closes the log when it can; a closed job returns zero counts.
    fn close(&mut self, reason: &str) -> Rolled {
        if !self.open {
            return Rolled::default();
        }
        self.open = false;
        let mut rolled = roll_back(self.inner.disk.as_ref(), &self.dir, &self.id, &self.records);
        if rolled.failed == 0 {
            let aborted = Record::Aborted {
                reason: reason.to_owned(),
            };
            rolled.closed = self.append(aborted).is_ok();
        }
        rolled
    }

    /// Writes one record and syncs it. After a failure the log is never written again;
    /// `in_doubt` tells a written but unsynced line from one never fully written.
    fn append(&mut self, record: Record) -> Result<(), WriteError> {
        if !self.log_ok {
            return Err(WriteError::Failed("the job log failed earlier".into()));
        }
        let line = encode(&record);
        if let Err(error) = self.log.write_all(line.as_bytes()) {
            self.log_ok = false;
            return Err(classify("write job log", &error));
        }
        #[cfg(test)]
        if matches!(record, Record::Backed { .. }) && SKIP_BACKED_SYNC.get() {
            self.records.push(record);
            return Ok(());
        }
        if let Err(error) = self.log.sync() {
            self.log_ok = false;
            self.in_doubt = true;
            return Err(classify("sync job log", &error));
        }
        self.records.push(record);
        Ok(())
    }
}

impl Drop for Job {
    /// A job left open is aborted, so its target never keeps half a write.
    fn drop(&mut self) {
        self.close("dropped");
    }
}
