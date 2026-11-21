//! Retention sweep (J4, NC141): run by `Journal::open` after recovery. It deletes leftover
//! `*.trash` folders, then removes complete jobs of dead owners older than 14 days, then the
//! oldest of them until all job folders hold at most 1 GiB. A job with no good record (a crash
//! inside `begin`) goes by age only (NC165). Open, damaged and live-owner jobs always stay.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

use sidewise_fs::durable::Disk;

use crate::Journal;
use crate::error::{JournalError, journal_io};
use crate::journal::{Inner, still_named};
use crate::record::{JobId, Record};
use crate::recover::{Found, read_job};

/// How long a complete job is kept: 14 days in ms (NC141).
pub const KEEP_MS: u64 = 14 * 86_400_000;
/// The most bytes all job folders may hold after the sweep: 1 GiB of logical size (NC141).
pub const CAP_BYTES: u64 = 1 << 30;
/// The suffix of a job folder taken out of the journal and being deleted.
const TRASH: &str = ".trash";

/// What the sweep did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Swept {
    /// Jobs taken out of the journal: their Undo is gone.
    pub removed: u32,
    /// Logical bytes of the job folders left, plus those of removals whose delete failed this
    /// time; unreadable folders and older trash not counted.
    pub bytes_left: u64,
    /// Folders that could not be read, owners that could not be checked, and removals that
    /// did not finish; each is kept or left as `*.trash` for the next start.
    pub failed: u32,
}

/// Which limits may remove a job folder.
enum Rule {
    /// Never removed: open, damaged, or with rewrite leftovers.
    Keep,
    /// No good record (NC165): removed by age only, never by the size cap or owner liveness.
    AgeOnly,
    /// Complete: removed by age or the cap when this owner is dead.
    Owner(String),
}

/// One job folder as the sweep read it.
struct Entry {
    id: JobId,
    created_ms: u64,
    bytes: u64,
    rule: Rule,
}

impl Journal {
    /// Applies NC141 retention at `now_ms` (ms since the Unix epoch) and returns what it did.
    /// Fails only when the journal folder cannot be listed; a job that fails is counted in
    /// `failed` and kept. The jobs of this journal's own owner are never removed.
    pub fn sweep(&self, now_ms: u64) -> Result<Swept, JournalError> {
        sweep(&self.inner, now_ms)
    }
}

/// The sweep over `inner.root`; see `Journal::sweep`.
fn sweep(inner: &Inner, now_ms: u64) -> Result<Swept, JournalError> {
    let disk = inner.disk.as_ref();
    let root = &inner.root;
    let mut swept = Swept::default();
    let mut entries = Vec::new();
    let names = disk
        .list(root)
        .map_err(|error| journal_io("list journal for the sweep", &error))?;
    for name in names {
        if name.ends_with(TRASH) {
            // `remove_dir_all` removes a symlink or junction itself, never what it points at.
            swept.failed += u32::from(disk.remove_dir_all(&root.join(&name)).is_err());
        } else if let Ok(id) = JobId::try_from(name) {
            match read_entry(disk, root, id) {
                Ok(entry) => entries.push(entry),
                Err(_) => swept.failed += 1,
            }
        }
    }
    entries.sort_by(|a, b| (a.created_ms, a.id.as_str()).cmp(&(b.created_ms, b.id.as_str())));
    let mut owners = Owners::new(inner);
    let mut total: u64 = entries.iter().map(|entry| entry.bytes).sum();
    for entry in &entries {
        let old = now_ms.saturating_sub(entry.created_ms) > KEEP_MS;
        let removable = match &entry.rule {
            Rule::Keep => false,
            Rule::AgeOnly => old,
            Rule::Owner(owner) => (old || total > CAP_BYTES) && owners.is_dead(owner, &mut swept),
        };
        if !removable {
            continue;
        }
        match remove_job(disk, root, &entry.id) {
            Removal::Kept => swept.failed += 1,
            Removal::Trashed => {
                swept.removed += 1;
                swept.failed += 1;
                total -= entry.bytes;
                swept.bytes_left += entry.bytes;
            }
            Removal::Gone => {
                swept.removed += 1;
                total -= entry.bytes;
            }
        }
    }
    swept.bytes_left += total;
    Ok(swept)
}

/// Reads a job folder's rule, age and logical bytes. Fails when the folder or a file in it
/// cannot be read, or the folder is a symlink or junction, so it is kept and not counted.
fn read_entry(disk: &dyn Disk, root: &Path, id: JobId) -> io::Result<Entry> {
    let dir = root.join(id.as_str());
    // `meta` refuses a link on Linux and Windows; `list` alone would read through it.
    disk.meta(&dir)?;
    let mut bytes = 0;
    for name in disk.list(&dir)? {
        bytes += disk.meta(&dir.join(name))?.map_or(0, |meta| meta.size);
    }
    let (rule, created_ms) = match read_job(disk, dir, id.clone())? {
        Found::Empty => (Rule::AgeOnly, id_created_ms(&id)),
        Found::Job(owner, job) if job.complete() && !job.leftover => match job.records.first() {
            Some(Record::Begin { created_ms, .. }) => (Rule::Owner(owner), *created_ms),
            _ => (Rule::Keep, 0),
        },
        Found::Job(..) | Found::Damaged => (Rule::Keep, 0),
    };
    Ok(Entry {
        id,
        created_ms,
        bytes,
        rule,
    })
}

/// The creation time in a job id (`{created_ms:013}-{pid}-{seq}`); `u64::MAX` (never old)
/// when it does not parse.
fn id_created_ms(id: &JobId) -> u64 {
    let time = id.as_str().split('-').next().unwrap_or_default();
    time.parse().unwrap_or(u64::MAX)
}

/// How far a removal got.
enum Removal {
    /// The rename or its sync failed: the job is still in the journal.
    Kept,
    /// Out of the journal as `<job>.trash`, but not deleted; the next sweep deletes it.
    Trashed,
    /// Deleted.
    Gone,
}

/// Removes a job folder: renames it to `<job>.trash`, syncs the journal folder so the rename
/// is durable before any file goes, then deletes the trash folder.
fn remove_job(disk: &dyn Disk, root: &Path, id: &JobId) -> Removal {
    let trash = root.join(format!("{}{TRASH}", id.as_str()));
    if disk.rename_new(&root.join(id.as_str()), &trash).is_err() {
        return Removal::Kept;
    }
    if disk.sync_dir(root).is_err() {
        return Removal::Trashed;
    }
    match disk.remove_dir_all(&trash) {
        Ok(()) => Removal::Gone,
        Err(_) => Removal::Trashed,
    }
}

/// The owners the sweep checked. A dead owner's lock file, when it has one, stays locked by
/// the sweep until it ends, so no process can start under that owner meanwhile.
struct Owners<'a> {
    dir: PathBuf,
    me: &'a str,
    /// `Some` for a dead owner (with its held lock, if a lock file exists); `None` for live.
    seen: BTreeMap<String, Option<Option<File>>>,
}

impl<'a> Owners<'a> {
    fn new(inner: &'a Inner) -> Self {
        Self {
            dir: inner.root.join("owners"),
            me: &inner.owner,
            seen: BTreeMap::new(),
        }
    }

    /// True when `owner` is dead. This journal's owner is live. A check that fails counts in
    /// `swept.failed` and is taken as live (fail closed).
    fn is_dead(&mut self, owner: &str, swept: &mut Swept) -> bool {
        if owner == self.me {
            return false;
        }
        if let Some(found) = self.seen.get(owner) {
            return found.is_some();
        }
        let lock = self.dir.join(format!("{owner}.lock"));
        let found = probe(&lock).unwrap_or_else(|_| {
            swept.failed += 1;
            None
        });
        let dead = found.is_some();
        self.seen.insert(owner.to_owned(), found);
        dead
    }
}

/// Checks an owner lock file without creating it: `Some(None)` when it is missing (recovery
/// deletes a dead owner's lock; a live owner holds its own), `Some(Some(file))` when we took
/// the lock, `None` when it is held or the name changed under us. Fails when it cannot open,
/// lock or identify the file.
fn probe(lock: &Path) -> Result<Option<Option<File>>, JournalError> {
    let file = match OpenOptions::new().write(true).open(lock) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Some(None)),
        opened => opened.map_err(|error| journal_io("open owner lock", &error))?,
    };
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Ok(None),
        Err(TryLockError::Error(error)) => return Err(journal_io("lock owner file", &error)),
    }
    Ok(still_named(&file, lock)?.then_some(Some(file)))
}

#[cfg(test)]
#[path = "sweep_tests.rs"]
mod tests;
