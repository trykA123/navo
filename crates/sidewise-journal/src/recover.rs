//! Recovery at open (J3, NC145): rolls back every incomplete job of a dead owner, closes its log
//! by an atomic rewrite (NC164), and removes the lock files of dead owners with no open job.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sidewise_fs::durable::{Disk, ReplaceError};

use crate::error::{JournalError, journal_io};
use crate::files::remove_if_present;
use crate::journal::{Inner, still_named};
use crate::record::{JobId, JobKind, Record, decode, encode};
use crate::rollback::roll_back;
use crate::sweep::Swept;

/// The job log, the rewrite that closes it, and where a failed Windows replace moves it.
const LOG: &str = "log";
const NEW_LOG: &str = "log.sw-new";
const OLD_LOG: &str = "log.sw-old";

/// What recovery at open did: jobs rolled back, items restored, kept or left for the next
/// start, and damaged logs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Recovered {
    /// Open jobs that recovery closed as `Aborted`, whatever each item's outcome (an item killed
    /// before its target changed restores nothing, yet the job was still rolled back).
    pub rolled_back: u32,
    /// The `Undo` jobs among `rolled_back`: their targets are back as the copy left them.
    pub rolled_back_undos: u32,
    /// Targets put back to their old bytes and modified time.
    pub restored: u32,
    /// The targets among `restored` that rolling back an `Undo` job put back.
    pub restored_undos: u32,
    /// Targets changed by someone else since the write: file and backup are both kept.
    pub kept: u32,
    /// Items, logs or jobs that could not be settled now; the next start retries them.
    pub failed: u32,
    /// Job logs with a bad complete line: left untouched, folder and backups kept.
    pub damaged: u32,
    /// What the retention sweep after recovery did; a `failed` count that stays above zero on
    /// every start means the journal may grow past its cap (NC141).
    pub swept: Swept,
}

/// A job folder as recovery reads it, before any write.
pub(crate) enum Found {
    /// No good record: the job never wrote (a crash in `begin`), and its owner is unknown.
    Empty,
    /// A bad line, a `Begin` that does not match the folder, or an unsafe target.
    Damaged,
    /// A readable log of the owner `<pid>-<start_ms>`.
    Job(String, Job),
}

/// One readable job log of a known owner.
pub(crate) struct Job {
    id: JobId,
    dir: PathBuf,
    pub(crate) records: Vec<Record>,
    /// The log bytes up to the end of the last good line (`Log::good_len`).
    good: Vec<u8>,
    /// The log name is gone and the log sits at `log.sw-old` (Windows 1176/1177).
    moved: bool,
    /// A rewrite leftover (`log.sw-new` or `log.sw-old`) is in the folder.
    pub(crate) leftover: bool,
}

/// A job folder as the listing saw it: only where it is. Its log is read again after the
/// owner is claimed, since a live owner may still commit or abort it before the claim.
struct Listed {
    id: JobId,
    dir: PathBuf,
    /// The log was complete at the listing; a complete job stays complete.
    complete: bool,
}

impl Job {
    /// True when the log holds `Commit` or `Aborted`: the job is never written again.
    pub(crate) fn complete(&self) -> bool {
        self.records
            .iter()
            .any(|record| matches!(record, Record::Commit | Record::Aborted { .. }))
    }

    /// True when the log's `Begin` names an `Undo` job.
    fn is_undo(&self) -> bool {
        matches!(
            self.records.first(),
            Some(Record::Begin {
                kind: JobKind::Undo,
                ..
            })
        )
    }
}

/// Recovers the journal under `inner.root`: reads every job, then, owner by owner, claims the
/// owner's lock and settles its jobs. Live owners and this process are skipped. `between` runs
/// after the listing and before the first claim (a test hook). Fails only when a folder
/// cannot be listed; a job that fails is counted and left for the next start.
pub(crate) fn recover_with(
    inner: &Inner,
    between: impl FnOnce(),
) -> Result<Recovered, JournalError> {
    let disk = inner.disk.as_ref();
    let mut recovered = Recovered::default();
    let mut owners: BTreeMap<String, Vec<Listed>> = BTreeMap::new();
    let names = disk
        .list(&inner.root)
        .map_err(|error| journal_io("list journal", &error))?;
    for name in names {
        // `owners`, J4's `*.trash` and foreign names are not jobs.
        let Ok(id) = JobId::try_from(name) else {
            continue;
        };
        let dir = inner.root.join(id.as_str());
        match read_job(disk, dir, id) {
            Ok(Found::Empty) => {}
            Ok(Found::Damaged) => recovered.damaged += 1,
            Ok(Found::Job(owner, job)) => owners.entry(owner).or_default().push(Listed {
                complete: job.complete(),
                id: job.id,
                dir: job.dir,
            }),
            Err(_) => recovered.failed += 1,
        }
    }
    let owners_dir = inner.root.join("owners");
    let locks = disk
        .list(&owners_dir)
        .map_err(|error| journal_io("list owners", &error))?;
    for stem in locks.iter().filter_map(|name| name.strip_suffix(".lock")) {
        if is_owner(stem) {
            owners.entry(stem.to_owned()).or_default();
        }
    }
    owners.remove(&inner.owner);
    between();
    for (owner, jobs) in &owners {
        let lock = owners_dir.join(format!("{owner}.lock"));
        settle_owner(disk, &lock, jobs, &mut recovered);
    }
    Ok(recovered)
}

/// Reads one job folder without writing: its log, or the log a failed replace moved to
/// `log.sw-old`. Fails when the folder or log cannot be read.
pub(crate) fn read_job(disk: &dyn Disk, dir: PathBuf, id: JobId) -> io::Result<Found> {
    let names = disk.list(&dir)?;
    let has = |name: &str| names.iter().any(|found| found == name);
    let moved = !has(LOG) && has(OLD_LOG);
    if !has(LOG) && !moved {
        // A rewrite with no log means the log vanished: fail closed.
        return Ok(if has(NEW_LOG) {
            Found::Damaged
        } else {
            Found::Empty
        });
    }
    let mut bytes = Vec::new();
    let path = dir.join(if moved { OLD_LOG } else { LOG });
    disk.open_read(&path)?.read_to_end(&mut bytes)?;
    let log = decode(&bytes);
    let owner = match log.records.first() {
        _ if log.damaged => return Ok(Found::Damaged),
        None => return Ok(Found::Empty),
        Some(Record::Begin { job, owner, .. }) if *job == id && is_owner(owner) => owner.clone(),
        Some(_) => return Ok(Found::Damaged),
    };
    // A relative target would resolve against our working folder: fail closed.
    let unsafe_target = log.records.iter().any(|record| {
        matches!(record, Record::Intent { target, .. } if !Path::new(target).is_absolute())
    });
    if unsafe_target {
        return Ok(Found::Damaged);
    }
    bytes.truncate(log.good_len);
    let job = Job {
        id,
        dir,
        records: log.records,
        good: bytes,
        moved,
        leftover: has(NEW_LOG) || has(OLD_LOG),
    };
    Ok(Found::Job(owner, job))
}

/// True for an owner stem `<pid>-<start_ms>`: two runs of 1 to 20 digits, so it is a safe name.
fn is_owner(stem: &str) -> bool {
    let digits =
        |part: &str| (1..=20).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit());
    stem.split_once('-')
        .is_some_and(|(pid, start)| digits(pid) && digits(start))
}

/// Claims the owner lock file `lock` (`<owner>.lock`), then reads each listed job again and
/// settles it; when every job ends complete, deletes the lock file. Skips a live owner. A lock
/// that cannot be checked fails closed: the jobs open at the listing count as failed and
/// nothing is touched.
fn settle_owner(disk: &dyn Disk, lock: &Path, jobs: &[Listed], recovered: &mut Recovered) {
    let owner = lock.file_stem().and_then(|stem| stem.to_str());
    let _held = match claim(lock) {
        Ok(Some(lock)) => lock,
        Ok(None) => return,
        Err(_) => {
            let open = jobs.iter().filter(|job| !job.complete).count();
            recovered.failed += u32::try_from(open).unwrap_or(u32::MAX);
            return;
        }
    };
    let mut settled = true;
    for listed in jobs {
        // Read only now, under the claim: the copy read at the listing may be stale.
        settled &= match read_job(disk, listed.dir.clone(), listed.id.clone()) {
            Ok(Found::Job(found, job)) if owner == Some(found.as_str()) => {
                settle_job(disk, &job, recovered)
            }
            Ok(Found::Job(..) | Found::Damaged) => {
                recovered.damaged += 1;
                true
            }
            Ok(Found::Empty) => true,
            Err(_) => {
                recovered.failed += 1;
                false
            }
        };
    }
    // While we hold the lock no one else deletes or recreates the name. A failed delete only
    // leaves a lock file of a dead owner: the next start tries again.
    if settled
        && remove_if_present(disk, lock).is_ok()
        && let Some(owners_dir) = lock.parent()
    {
        let _ = disk.sync_dir(owners_dir);
    }
}

/// Takes the lock on a dead owner's lock file, creating the file if it is gone, so no other
/// recovery acts on the owner's jobs meanwhile. `None` when the lock is held (a live owner or
/// another recovery) or the name changed before the lock. Fails when the file cannot be
/// opened or locked, or its identity is unknown (fail closed).
fn claim(path: &Path) -> Result<Option<File>, JournalError> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| journal_io("open owner lock", &error))?;
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Ok(None),
        Err(TryLockError::Error(error)) => return Err(journal_io("lock owner file", &error)),
    }
    Ok(still_named(&file, path)?.then_some(file))
}

/// Settles one job of a claimed dead owner: puts its log back in place and drops rewrite
/// leftovers, then, if the job is open, rolls it back (NC145) and closes its log. Returns
/// true when the job ends complete; counts what it did in `recovered`.
fn settle_job(disk: &dyn Disk, job: &Job, recovered: &mut Recovered) -> bool {
    if job.leftover && put_back_log(disk, job).is_err() {
        recovered.failed += 1;
        return false;
    }
    if job.complete() {
        return true;
    }
    let rolled = roll_back(disk, &job.dir, &job.id, &job.records);
    let undo = job.is_undo();
    recovered.restored += rolled.restored;
    if undo {
        recovered.restored_undos += rolled.restored;
    }
    recovered.kept += rolled.kept;
    recovered.failed += rolled.failed;
    if rolled.failed != 0 {
        return false;
    }
    if close_log(disk, &job.dir, &job.good).is_err() {
        recovered.failed += 1;
        return false;
    }
    recovered.rolled_back += 1;
    recovered.rolled_back_undos += u32::from(undo);
    true
}

/// Undoes what an interrupted rewrite left (NC164): renames a moved-aside log back, deletes
/// `log.sw-new` and `log.sw-old`, then syncs the folder. Fails when a step fails.
fn put_back_log(disk: &dyn Disk, job: &Job) -> io::Result<()> {
    let at = |name: &str| job.dir.join(name);
    if job.moved {
        disk.rename_new(&at(OLD_LOG), &at(LOG))?;
    }
    remove_if_present(disk, &at(NEW_LOG))?;
    remove_if_present(disk, &at(OLD_LOG))?;
    disk.sync_dir(&job.dir)
}

/// Closes a job log by an atomic rewrite (NC164): `log.sw-new` gets the good lines plus
/// `Aborted {recovered}` and is synced, then replaces `log`, then the folder is synced.
/// Fails when a step fails; the next recovery deletes the leftovers and tries again.
fn close_log(disk: &dyn Disk, dir: &Path, good: &[u8]) -> io::Result<()> {
    let new = dir.join(NEW_LOG);
    let aborted = Record::Aborted {
        reason: "recovered".to_owned(),
    };
    let mut file = disk.create_new(&new)?;
    file.write_all(good)?;
    file.write_all(encode(&aborted).as_bytes())?;
    file.sync()?;
    drop(file);
    disk.replace(&new, &dir.join(LOG), &dir.join(OLD_LOG))
        .map_err(|error| match error {
            ReplaceError::Unchanged(error)
            | ReplaceError::NotDurable(error)
            | ReplaceError::TargetMoved(error) => error,
        })?;
    disk.sync_dir(dir)
}
