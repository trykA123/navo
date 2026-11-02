use super::presence;
use super::write::{flush_folder, kept_back, put_back_flushed};
use crate::job::copy::failure;
use crate::job::restore::is_placed;
use crate::{HeldFile, JobContext, StepOutcome};
use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};

const CURRENT_SUFFIX: &str = ".current";

pub(in crate::job) fn undo_write(
    held: &HeldFile,
    ctx: &mut JobContext<'_>,
) -> Result<StepOutcome, FsError> {
    let ops = ctx.ops;
    let Some(placed_hash) = held.placed_hash else {
        return Err(failure(FsErrorKind::ChangedOnDisk, &held.original));
    };
    if presence(ops, &held.held)?.is_none() {
        return Err(failure(FsErrorKind::NotFound, &held.held));
    }
    let Some(entry) = presence(ops, &held.original)? else {
        return kept_back(ctx, held);
    };
    if !is_placed(&entry, held) {
        return Err(failure(FsErrorKind::ChangedOnDisk, &held.original));
    }
    let current = HeldFile {
        original: held.original.clone(),
        held: beside(&held.held)?,
        placed: held.placed,
        volume: held.volume,
        placed_hash: None,
    };
    if !ctx.log.journal_held(&current)? {
        return Err(failure(FsErrorKind::NotSupportedYet, &held.original));
    }
    ops.rename(&current.original, &current.held)?;
    if let Err(cause) = check_current(ctx, &current, placed_hash) {
        return Err(put_back_flushed(ctx, &current, cause));
    }
    ops.rename(&held.held, &held.original)
        .map_err(|cause| put_back_flushed(ctx, &current, cause))?;
    flush_folder(ctx, &held.held)?;
    flush_folder(ctx, &held.original)?;
    Ok(StepOutcome::Done)
}

fn check_current(
    ctx: &JobContext<'_>,
    current: &HeldFile,
    placed_hash: [u8; 32],
) -> Result<(), FsError> {
    flush_folder(ctx, &current.held)?;
    flush_folder(ctx, &current.original)?;
    let entry = ctx.ops.metadata(&current.held)?;
    if !is_placed(&entry, current)
        || ctx.ops.content_hash(&current.held, ctx.cancel)? != placed_hash
    {
        return Err(failure(FsErrorKind::ChangedOnDisk, &current.original));
    }
    if ctx.cancel.is_cancelled() {
        return Err(failure(FsErrorKind::Cancelled, &current.original));
    }
    Ok(())
}

fn beside(held: &Location) -> Result<Location, FsError> {
    let invalid = || failure(FsErrorKind::NameInvalid, held);
    let name = held.name().ok_or_else(invalid)?.to_text();
    let folder = held.parent().ok_or_else(invalid)?;
    folder
        .join(&ProviderPath::from_text(&format!("{name}{CURRENT_SUFFIX}")))
        .map_err(|_| invalid())
}
