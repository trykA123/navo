use super::{Aside, make_room};
use crate::job::copy::failure;
use crate::job::restore::{Restored, keep};
use crate::job::undo::settle;
use crate::job::write_file::{Checked, Write, check_staged};
use crate::{FileStamp, HeldFile, JobContext, StepOutcome};
use rusty_core::{FsError, FsErrorKind, Location};

impl Aside {
    pub(in crate::job) fn write_over(
        &self,
        ctx: &mut JobContext<'_>,
        write: &Write<'_>,
        checked: &Checked,
    ) -> Result<(), FsError> {
        let ops = ctx.ops;
        let placed = Some(FileStamp {
            size: checked.staged.size,
            modified_ms: checked.staged.modified_ms,
        });
        let held = HeldFile {
            placed_hash: Some(checked.placed_hash),
            ..self.held_file(ops, &self.on_disk(ops, write.target), placed)?
        };
        if !ctx.log.journal_held(&held)? {
            return Err(failure(FsErrorKind::NotSupportedYet, write.target));
        }
        make_room(ops, &held.held)?;
        ops.rename(&held.original, &held.held)?;
        if let Err(cause) = check_hold(ctx, (write, checked), &held) {
            return Err(put_back_flushed(ctx, &held, cause));
        }
        ops.rename(write.staged, write.target)
            .map_err(|cause| put_back_flushed(ctx, &held, cause))?;
        flush_folder(ctx, write.target)
    }
}

fn check_hold(
    ctx: &JobContext<'_>,
    (write, checked): (&Write<'_>, &Checked),
    held: &HeldFile,
) -> Result<(), FsError> {
    let ops = ctx.ops;
    flush_folder(ctx, &held.held)?;
    flush_folder(ctx, write.target)?;
    let changed = || failure(FsErrorKind::ChangedOnDisk, write.target);
    let old = ops.metadata(&held.held)?;
    if old.size != write.expect.size
        || old.modified_ms != write.expect.modified_ms
        || ops.file_identity(&held.held)? != checked.identity
        || ops.content_hash(&held.held, ctx.cancel)? != write.expect.hash
    {
        return Err(changed());
    }
    if ctx.cancel.is_cancelled() {
        return Err(failure(FsErrorKind::Cancelled, write.target));
    }
    let staged = check_staged(ops, write)?;
    if staged.modified_ms != checked.staged.modified_ms {
        return Err(failure(FsErrorKind::ChangedOnDisk, write.staged));
    }
    Ok(())
}

pub(in crate::job) fn kept_back(
    ctx: &mut JobContext<'_>,
    held: &HeldFile,
) -> Result<StepOutcome, FsError> {
    let restored = keep(ctx.ops, held)?;
    if !matches!(restored, Restored::Gone) {
        flush_both(ctx, held)?;
    }
    settle(restored, held, ctx)
}

pub(super) fn put_back_flushed(
    ctx: &mut JobContext<'_>,
    held: &HeldFile,
    cause: FsError,
) -> FsError {
    let flushed = match keep(ctx.ops, held) {
        Ok(Restored::Gone) => return cause,
        Ok(restored) => flush_both(ctx, held).map(|()| restored),
        Err(error) => Err(error),
    };
    match flushed {
        Ok(Restored::Beside(beside)) => {
            ctx.log.kept_beside(held, &beside);
            cause
        }
        Ok(_) => cause,
        Err(error) => {
            ctx.log.held_stranded(held);
            error
        }
    }
}

fn flush_both(ctx: &JobContext<'_>, held: &HeldFile) -> Result<(), FsError> {
    flush_folder(ctx, &held.held)?;
    flush_folder(ctx, &held.original)
}

pub(super) fn flush_folder(ctx: &JobContext<'_>, at: &Location) -> Result<(), FsError> {
    let folder = at
        .parent()
        .ok_or_else(|| failure(FsErrorKind::NameInvalid, at))?;
    ctx.ops.flush(&folder)
}
