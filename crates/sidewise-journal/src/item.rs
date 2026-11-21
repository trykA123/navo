//! The steps of one Overwrite item (NC144) that touch only the target and its files: the
//! pre-check with the NC147 refusals, staging, the backup and the same-file check (NC160).

use std::io::{self, Write};
use std::path::Path;

use navo_core::{FileStamp, Location};
use sidewise_fs::durable::{Disk, Meta, ReplaceError};
use sidewise_fs::{DriveClass, drive_class};

use crate::error::{WriteError, classify};
use crate::files::{Hashing, copy_hashed, hash_file, set_time_closed};

/// What the caller last saw of a target; `None` fields are not checked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Expect {
    /// Length in bytes.
    pub size: Option<u64>,
    /// Last write time in ms since the Unix epoch.
    pub modified_ms: Option<i64>,
    /// Blake3 hash of the bytes; the backup copy must match it before the replace.
    pub hash: Option<[u8; 32]>,
}

/// The fill callback of `Job::overwrite`: writes the new bytes, or fails with a reason.
pub type Fill<'a> = &'a mut dyn FnMut(&mut dyn Write) -> Result<(), String>;

/// One overwrite as the caller asked for it.
pub(crate) struct Request<'a> {
    pub(crate) target: &'a Path,
    pub(crate) expect: Expect,
    /// The modified time the new file gets; `None` keeps the write time.
    pub(crate) restore_mtime_ms: Option<i64>,
}

/// NC144 step 8 errors as an item error: every case rolls back by hash (NC145).
pub(crate) fn replace_error(error: ReplaceError) -> WriteError {
    match error {
        ReplaceError::Unchanged(error)
        | ReplaceError::NotDurable(error)
        | ReplaceError::TargetMoved(error) => classify("replace target", &error),
    }
}

/// NC144 step 1 with the NC147 refusals: an absolute UTF-8 path, not on a network drive, a
/// regular file that we may write, with a known identity (unless the volume keeps no stable
/// ids, NC202) and the expected size and time.
/// Returns what it saw; fails before anything is written.
pub(crate) fn precheck(disk: &dyn Disk, request: &Request<'_>) -> Result<Meta, WriteError> {
    let (target, expect) = (request.target, request.expect);
    let not_copyable = |why: &str| WriteError::Failed(format!("not copyable: {why}"));
    let text = target
        .to_str()
        .ok_or_else(|| not_copyable("the path is not UTF-8"))?;
    if !target.is_absolute() {
        return Err(not_copyable("the path is not absolute"));
    }
    let location = Location::new(text).map_err(|error| not_copyable(&error.to_string()))?;
    if drive_class(&location) == DriveClass::Network {
        return Err(WriteError::Network);
    }
    let found = disk
        .meta(target)
        .map_err(|error| classify("check target", &error))?;
    let meta = found.ok_or(WriteError::Changed)?;
    if meta.read_only {
        return Err(WriteError::ReadOnly);
    }
    if meta.identity.is_none() && !meta.no_stable_ids {
        return Err(WriteError::Failed("the file identity is unknown".into()));
    }
    let size_moved = expect.size.is_some_and(|size| size != meta.size);
    let time_moved = expect
        .modified_ms
        .is_some_and(|ms| Some(ms) != meta.modified_ms);
    if size_moved || time_moved {
        return Err(WriteError::Changed);
    }
    Ok(meta)
}

/// NC144 step 3: fills the staged file, applies the target's mode, syncs and closes it, then
/// sets the restore time (and syncs again), and returns its stamp. A fill error maps back to
/// the OS error it hit, if any.
pub(crate) fn stage(
    disk: &dyn Disk,
    staged: &Path,
    request: &Request<'_>,
    fill: Fill<'_>,
) -> Result<FileStamp, WriteError> {
    let io_err = |step: &'static str| move |error: io::Error| classify(step, &error);
    let mut out = Hashing::new(
        disk.create_new(staged)
            .map_err(io_err("make staged file"))?,
    );
    if let Err(reason) = fill(&mut out) {
        return Err(match out.error.take() {
            Some(error) => classify("fill staged file", &error),
            None => WriteError::Failed(reason),
        });
    }
    let restore = request.restore_mtime_ms;
    if restore.is_none() {
        disk.copy_mode(request.target, staged)
            .map_err(io_err("copy mode"))?;
    }
    out.file.sync().map_err(io_err("sync staged file"))?;
    let (size, hash) = (out.len, *out.hasher.finalize().as_bytes());
    drop(out);
    if let Some(ms) = restore {
        set_time_closed(disk, staged, ms, Some(request.target))
            .map_err(io_err("set staged time"))?;
    }
    let meta = disk.meta(staged).map_err(io_err("read staged file"))?;
    Ok(FileStamp {
        size,
        modified_ms: meta.and_then(|meta| meta.modified_ms),
        hash,
    })
}

/// NC144 step 5: checks the request's target still matches `before`, then copies it to
/// `backup` while hashing it and syncs the copy. Fails with `Changed` when the target moved,
/// or the copy's length is not `before`'s or its hash not the expected one: nothing is replaced.
pub(crate) fn back_up(
    disk: &dyn Disk,
    request: &Request<'_>,
    backup: &Path,
    before: &Meta,
) -> Result<FileStamp, WriteError> {
    let (target, want_hash) = (request.target, request.expect.hash);
    check_same(
        before,
        disk.meta(target)
            .map_err(|e| classify("check target", &e))?,
    )?;
    let (len, hash) = copy_hashed(disk, target, backup, || Ok(()))
        .map_err(|error| classify("back up target", &error))?;
    if len != before.size || want_hash.is_some_and(|want| want != hash) {
        return Err(WriteError::Changed);
    }
    Ok(FileStamp {
        size: len,
        modified_ms: before.modified_ms,
        hash,
    })
}

/// Fails with `Changed` unless `now` is the same file as `before`: same size, modified time
/// and identity, read by path (NC160).
pub(crate) fn check_same(before: &Meta, now: Option<Meta>) -> Result<(), WriteError> {
    let same = now.is_some_and(|now| {
        now.size == before.size
            && now.modified_ms == before.modified_ms
            && now.identity == before.identity
    });
    if same {
        Ok(())
    } else {
        Err(WriteError::Changed)
    }
}

/// The last check before a replace: true when `target` is still `before` by size, time and
/// identity (NC160). With no identity (a volume without stable ids, NC202) it must also still
/// hash to `hash`, and its size and time are read again after the hash. Fails when it cannot
/// be read, so the caller stops (fail closed).
pub(crate) fn still_same(
    disk: &dyn Disk,
    target: &Path,
    before: &Meta,
    hash: &[u8; 32],
) -> io::Result<bool> {
    if check_same(before, disk.meta(target)?).is_err() {
        return Ok(false);
    }
    if before.identity.is_some() {
        return Ok(true);
    }
    let same_hash = hash_file(disk, target)? == Some(*hash);
    Ok(same_hash && check_same(before, disk.meta(target)?).is_ok())
}
