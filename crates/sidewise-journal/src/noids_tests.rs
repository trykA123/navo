//! Tests of overwrites on volumes without stable file ids (exFAT, FAT; NC202): there the target
//! is re-hashed just before the replace instead of matched by identity, in a job, an undo, a
//! rollback and the crash loops. A volume with stable ids still refuses an unknown identity.
// Every platform; with SIDEWISE_NO_IDS_DIR set to a folder on exFAT, the real-disk case runs there.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use sidewise_fs::durable::{Disk, DiskFile, Meta, RealDisk, ReplaceError};

use crate::{Expect, JobKind, Journal, WriteError};

pub(crate) const OLD: &[u8] = b"old bytes";
/// Other bytes of the same length as `OLD`.
const SWAP: &[u8] = b"his bytes";
pub(crate) const NEW: &[u8] = b"the new bytes";
/// On a 2 s boundary, so exFAT's write-time granularity keeps it exact.
const OLD_MS: i64 = 1_700_000_002_000;

/// A disk over `inner` that reports no identity, like exFAT (or, with `Ids::Kept`, the real
/// one). With `swap`, it plays a same-size, same-time edit; `replaced` counts the replaces
/// of the swapped path after the edit.
pub(crate) struct NoIds {
    inner: Arc<dyn Disk>,
    ids: Ids,
    swap: Option<Swap>,
    state: Mutex<SwapState>,
}

/// What `NoIds` reports as the identity of a file.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ids {
    /// The inner disk's identity and volume flag.
    Kept,
    /// None, on a volume without stable ids.
    Hidden,
    /// None, though the volume claims stable ids (an unknown identity there must be refused).
    HiddenOnStable,
}

/// Once armed, the first created file whose name passes `on` primes the swap; the next
/// `meta` of `path` then rewrites it in place with `bytes` and puts its modified time back.
pub(crate) struct Swap {
    pub(crate) path: PathBuf,
    pub(crate) bytes: &'static [u8],
    pub(crate) on: fn(&str) -> bool,
}

/// The progress of a `Swap`, and the replaces of its path after it happened.
#[derive(Default)]
pub(crate) struct SwapState {
    armed: bool,
    primed: bool,
    swapped: bool,
    pub(crate) replaced: usize,
}

/// True for a backup name, `<item>.bak`.
pub(crate) fn is_backup(name: &str) -> bool {
    name.ends_with(".bak")
}

/// `inner` on a volume without stable ids.
fn no_ids(inner: Arc<dyn Disk>) -> Arc<dyn Disk> {
    Arc::new(NoIds::new(inner, Ids::Hidden, None))
}

impl NoIds {
    /// A disk over `inner`; its swap waits for `arm`.
    pub(crate) fn new(inner: Arc<dyn Disk>, ids: Ids, swap: Option<Swap>) -> Self {
        Self {
            inner,
            ids,
            swap,
            state: Mutex::default(),
        }
    }

    /// Arms the swap: from now on, the next matching create primes it.
    pub(crate) fn arm(&self) {
        self.state().unwrap().armed = true;
    }

    /// The locked swap state.
    pub(crate) fn state(&self) -> io::Result<std::sync::MutexGuard<'_, SwapState>> {
        self.state.lock().map_err(|_| io::Error::other("poisoned"))
    }

    /// Rewrites the swap target once, if primed and `path` is it.
    fn maybe_swap(&self, path: &Path) -> io::Result<()> {
        let Some(swap) = self.swap.as_ref().filter(|swap| swap.path == path) else {
            return Ok(());
        };
        let mut state = self.state()?;
        if state.primed && !state.swapped {
            state.swapped = true;
            let ms = RealDisk.meta(path)?.and_then(|meta| meta.modified_ms);
            fs::write(path, swap.bytes)?;
            RealDisk.set_modified(path, ms.ok_or_else(|| io::Error::other("no mtime"))?)?;
        }
        Ok(())
    }
}

impl Disk for NoIds {
    fn create_new(&self, path: &Path) -> io::Result<Box<dyn DiskFile>> {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if let Some(swap) = &self.swap {
            let mut state = self.state()?;
            if state.armed && !state.primed && (swap.on)(&name) {
                state.primed = true;
            }
        }
        self.inner.create_new(path)
    }
    fn open_append(&self, path: &Path) -> io::Result<Box<dyn DiskFile>> {
        self.inner.open_append(path)
    }
    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        self.inner.open_read(path)
    }
    fn meta(&self, path: &Path) -> io::Result<Option<Meta>> {
        self.maybe_swap(path)?;
        Ok(self.inner.meta(path)?.map(|meta| match self.ids {
            Ids::Kept => meta,
            hidden => Meta {
                identity: None,
                no_stable_ids: hidden == Ids::Hidden,
                ..meta
            },
        }))
    }
    fn set_modified(&self, path: &Path, ms: i64) -> io::Result<()> {
        self.inner.set_modified(path, ms)
    }
    fn copy_mode(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.copy_mode(from, to)
    }
    fn replace(&self, staged: &Path, target: &Path, old: &Path) -> Result<(), ReplaceError> {
        if self.swap.as_ref().is_some_and(|swap| swap.path == target) {
            let mut state = self.state().map_err(ReplaceError::Unchanged)?;
            if state.swapped {
                state.replaced += 1;
            }
        }
        self.inner.replace(staged, target, old)
    }
    fn rename_new(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename_new(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_file(path)
    }
    fn create_dir(&self, path: &Path) -> io::Result<()> {
        self.inner.create_dir(path)
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_dir_all(path)
    }
    fn list(&self, dir: &Path) -> io::Result<Vec<String>> {
        self.inner.list(dir)
    }
    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        self.inner.sync_dir(dir)
    }
}

/// A journal folder and `count` targets holding `OLD` at `OLD_MS`; the targets go in `at`
/// when given, else beside the journal.
pub(crate) struct Files {
    pub(crate) journal: tempfile::TempDir,
    _targets: tempfile::TempDir,
    pub(crate) targets: Vec<PathBuf>,
}

pub(crate) fn files(count: usize, at: Option<&Path>) -> Files {
    let journal = tempfile::tempdir().unwrap();
    let dir = match at {
        Some(at) => tempfile::tempdir_in(at).unwrap(),
        None => tempfile::tempdir().unwrap(),
    };
    let targets: Vec<PathBuf> = (0..count)
        .map(|i| dir.path().join(format!("{i}.txt")))
        .collect();
    for target in &targets {
        fs::write(target, OLD).unwrap();
        RealDisk.set_modified(target, OLD_MS).unwrap();
    }
    Files {
        journal,
        _targets: dir,
        targets,
    }
}

/// The bytes and modified time of `path`.
pub(crate) fn state(path: &Path) -> (Vec<u8>, Option<i64>) {
    let meta = RealDisk.meta(path).unwrap().unwrap();
    (fs::read(path).unwrap(), meta.modified_ms)
}

/// The names in the targets' folder that end in the temp suffix.
pub(crate) fn temps(files: &Files) -> Vec<String> {
    let dir = files.targets[0].parent().unwrap();
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(navo_core::TEMP_SUFFIX))
        .collect()
}

pub(crate) fn write_new(out: &mut dyn Write) -> Result<(), String> {
    out.write_all(NEW).map_err(|error| error.to_string())
}

/// Overwrites every target with `NEW` in one committed job, then undoes it, checking each step.
fn overwrite_then_undo(disk: Arc<dyn Disk>, files: &Files) {
    let journal = Journal::open_raw(files.journal.path(), disk).unwrap();
    let mut job = journal.begin(JobKind::CopyMany, "Copy", None).unwrap();
    for target in &files.targets {
        job.overwrite(target, Expect::default(), None, &mut write_new)
            .unwrap();
    }
    let id = job.commit().unwrap();
    for target in &files.targets {
        assert_eq!(fs::read(target).unwrap(), NEW);
    }
    assert_eq!(journal.undo(&id).unwrap(), files.targets);
    for target in &files.targets {
        assert_eq!(state(target), (OLD.to_vec(), Some(OLD_MS)), "undone");
    }
    assert!(temps(files).is_empty());
}

#[test]
fn an_unchanged_target_without_ids_is_overwritten_and_undone() {
    let files = files(2, None);
    overwrite_then_undo(no_ids(Arc::new(RealDisk)), &files);
}

#[test]
fn a_real_disk_overwrite_and_undo_work_and_exfat_reports_no_ids() {
    let at = std::env::var_os("SIDEWISE_NO_IDS_DIR").map(PathBuf::from);
    let files = files(1, at.as_deref());
    if at.is_some() {
        let meta = RealDisk.meta(&files.targets[0]).unwrap().unwrap();
        assert_eq!((meta.identity, meta.no_stable_ids), (None, true));
    }
    overwrite_then_undo(Arc::new(RealDisk), &files);
}

#[test]
fn a_same_size_same_time_swap_before_the_replace_is_refused_by_hash() {
    let files = files(1, None);
    let target = files.targets[0].clone();
    let swap = Swap {
        path: target.clone(),
        bytes: SWAP,
        on: is_backup,
    };
    let disk = Arc::new(NoIds::new(Arc::new(RealDisk), Ids::Hidden, Some(swap)));
    disk.arm();
    let journal = Journal::open_raw(files.journal.path(), disk).unwrap();
    let mut job = journal.begin(JobKind::Copy, "Copy", None).unwrap();
    let result = job.overwrite(&target, Expect::default(), None, &mut write_new);
    assert_eq!(result, Err(WriteError::Changed));
    assert_eq!(
        state(&target),
        (SWAP.to_vec(), Some(OLD_MS)),
        "the edit is kept"
    );
    assert!(temps(&files).is_empty());
}

#[test]
fn an_unknown_identity_on_a_volume_with_stable_ids_is_still_refused() {
    let files = files(1, None);
    let disk = Arc::new(NoIds::new(Arc::new(RealDisk), Ids::HiddenOnStable, None));
    let journal = Journal::open_raw(files.journal.path(), disk).unwrap();
    let mut job = journal.begin(JobKind::Copy, "Copy", None).unwrap();
    let result = job.overwrite(&files.targets[0], Expect::default(), None, &mut write_new);
    assert_eq!(
        result,
        Err(WriteError::Failed("the file identity is unknown".into()))
    );
    assert_eq!(state(&files.targets[0]), (OLD.to_vec(), Some(OLD_MS)));
}

#[test]
fn a_failed_second_item_rolls_the_first_back_without_ids() {
    let files = files(2, None);
    let journal = Journal::open_raw(files.journal.path(), no_ids(Arc::new(RealDisk))).unwrap();
    let mut job = journal.begin(JobKind::CopyMany, "Copy", None).unwrap();
    job.overwrite(&files.targets[0], Expect::default(), None, &mut write_new)
        .unwrap();
    let mut fail = |_: &mut dyn Write| Err("no source".to_owned());
    let result = job.overwrite(&files.targets[1], Expect::default(), None, &mut fail);
    assert_eq!(result, Err(WriteError::Failed("no source".into())));
    for target in &files.targets {
        assert_eq!(state(target), (OLD.to_vec(), Some(OLD_MS)), "rolled back");
    }
    assert!(temps(&files).is_empty());
}

/// The job crash loops and the recovery at open, run on a volume without stable ids.
#[cfg(target_os = "linux")]
mod crash {
    use std::sync::Arc;

    use sidewise_fs::durable::{Disk, FaultDisk, Plan, RealDisk};

    use super::no_ids;
    use crate::Journal;
    use crate::job_crash_tests::{PATTERNS, assert_recoverable};
    use crate::job_tests::{Fixture, run};
    use crate::recover_crash_tests::check_settled;

    /// Crashes a two-item job at every op and pattern, then checks it and recovers it.
    fn loop_without_ids(commit: bool) {
        let pass = Arc::new(FaultDisk::new(RealDisk, Plan::Pass));
        let _ = run(no_ids(pass.clone()), &Fixture::new(2), commit);
        let ops = pass.ops();
        assert!(ops > 40, "the loop covers both items ({ops} ops)");
        for n in 1..=ops + 1 {
            for (name, keep, tail) in PATTERNS {
                let fixture = Fixture::new(2);
                let fault = Arc::new(FaultDisk::new(RealDisk, Plan::CrashAt(n)));
                let _ = run(no_ids(fault.clone()), &fixture, commit);
                fault.crash_with(keep, tail).unwrap();
                let case = format!("crash at op {n} of {ops}, {name}");
                assert_recoverable(&fixture, &case);
                let disk: Arc<dyn Disk> = no_ids(Arc::new(RealDisk));
                let (_journal, recovered) = Journal::open(&fixture.journal_dir(), disk).unwrap();
                assert_eq!((recovered.failed, recovered.damaged), (0, 0), "{case}");
                check_settled(&fixture, &case).unwrap();
            }
        }
    }

    #[test]
    fn a_crash_at_every_op_without_ids_is_recoverable_after_commit() {
        loop_without_ids(true);
    }

    #[test]
    fn a_crash_at_every_op_without_ids_is_recoverable_during_rollback() {
        loop_without_ids(false);
    }
}
