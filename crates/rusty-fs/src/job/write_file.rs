use rusty_core::{FsError, FsErrorKind, Location};

use super::aside::{Aside, kept_back, presence};
use super::copy::failure;
use super::restore::is_placed;
use super::{HeldFile, JobContext, StepOutcome};
use crate::{EntryKind, FileIdentity, FsEntry, ProviderOps};

pub(super) struct Write<'a> {
    pub target: &'a Location,
    pub staged: &'a Location,
    pub staged_len: u64,
    pub expect: &'a rusty_core::FileStamp,
}

pub(super) struct Checked {
    pub staged: FsEntry,
    pub identity: Option<FileIdentity>,
    pub placed_hash: [u8; 32],
}

pub(super) fn run(
    aside: &Aside,
    ctx: &mut JobContext<'_>,
    write: &Write<'_>,
) -> Result<StepOutcome, FsError> {
    if ctx.log.preserve_overwrites() {
        return Err(failure(FsErrorKind::NotSupportedYet, write.target));
    }
    let ops = ctx.ops;
    check_stamp(ops, write)?;
    let staged = check_staged(ops, write)?;
    if ops.content_hash(write.target, ctx.cancel)? != write.expect.hash {
        return Err(failure(FsErrorKind::ChangedOnDisk, write.target));
    }
    let checked = Checked {
        staged,
        identity: ops.file_identity(write.target)?,
        placed_hash: ops.content_hash(write.staged, ctx.cancel)?,
    };
    if ctx.cancel.is_cancelled() {
        return Err(failure(FsErrorKind::Cancelled, write.target));
    }
    ops.flush(write.staged)?;
    aside
        .write_over(ctx, write, &checked)
        .map(|()| StepOutcome::Done)
}

pub(super) fn check_staged(ops: &dyn ProviderOps, write: &Write<'_>) -> Result<FsEntry, FsError> {
    let staged = ops.metadata(write.staged)?;
    if staged.kind != EntryKind::File || staged.size != write.staged_len {
        return Err(failure(FsErrorKind::ChangedOnDisk, write.staged));
    }
    let folder = write
        .target
        .parent()
        .ok_or_else(|| failure(FsErrorKind::NameInvalid, write.target))?;
    let volume = ops.volume_serial(write.staged)?;
    let same = volume.is_some()
        && volume == ops.volume_serial(&folder)?
        && ops.mount_identity(write.staged)? == ops.mount_identity(&folder)?;
    if !same {
        return Err(failure(FsErrorKind::NotSupportedYet, write.staged));
    }
    Ok(staged)
}

fn check_stamp(ops: &dyn ProviderOps, write: &Write<'_>) -> Result<(), FsError> {
    let expect = write.expect;
    let same = presence(ops, write.target)?.is_some_and(|entry| {
        entry.kind == EntryKind::File
            && entry.size == expect.size
            && entry.modified_ms == expect.modified_ms
    });
    if !same {
        return Err(failure(FsErrorKind::ChangedOnDisk, write.target));
    }
    Ok(())
}

pub(super) fn recover(
    ctx: &mut JobContext<'_>,
    held: Option<&HeldFile>,
) -> Result<StepOutcome, FsError> {
    let Some(held) = held else {
        return Ok(StepOutcome::Skipped);
    };
    let ops = ctx.ops;
    if presence(ops, &held.held)?.is_some()
        && let Some(entry) = presence(ops, &held.original)?
    {
        if !was_placed(ctx, &entry, held)? {
            return Err(failure(FsErrorKind::ChangedOnDisk, &held.original));
        }
        return Ok(StepOutcome::Skipped);
    }
    kept_back(ctx, held)
}

fn was_placed(ctx: &JobContext<'_>, entry: &FsEntry, held: &HeldFile) -> Result<bool, FsError> {
    let Some(hash) = held.placed_hash else {
        return Ok(false);
    };
    Ok(is_placed(entry, held) && ctx.ops.content_hash(&held.original, ctx.cancel)? == hash)
}
