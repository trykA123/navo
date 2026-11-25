use std::ops::Range;

use navo_core::{FsError, FsErrorKind, Location, OpKind, OperationPlan, Step};

use super::aside::presence;
use super::copy::{legacy_temp_of, remove_if_present, temp_of};
use super::link::undo_link;
use super::marks::is_removal;
use super::restore::{
    Restored, gone_or_folder_deleted, is_placed, keep, matches_stamp, restore, surely_absent,
};
use super::rollback_lookup::Rollback;
use super::{FileStamp, HeldFile, JobContext, JobOutcome, Journaled, KeptCopy, StepOutcome};
use crate::{ProviderOps, already_exists};

pub(super) fn roll_back(job: &Journaled<'_>, ctx: &mut JobContext<'_>) -> JobOutcome {
    let run = Rollback::of(job);
    let steps: Vec<(u32, &Step)> = (0_u32..).zip(&job.plan.steps).collect();
    let mut first = None;
    for &(seq, step) in steps.iter().rev() {
        let held = held_for(step, &run);
        let unsure = job.unsure == Some(seq) && !run.is_done(seq) && held.is_none();
        if !run.is_done(seq) && held.is_none() && !unsure {
            continue;
        }
        if awaits_unreadable_row(&run, step, held) {
            continue;
        }
        let result = if is_plain_move(job.plan.kind, step, held) {
            super::moved::undo_move(ctx.ops, &*ctx.log, (seq, step))
        } else {
            checked_undo(&run, (seq, step), held, ctx)
        };
        let logged = match result {
            Ok(outcome) => ctx.log.undone(seq, outcome),
            Err(error) => {
                let _ = ctx.log.undone(seq, StepOutcome::Failed);
                Err(error)
            }
        };
        if let Err(error) = logged {
            first.get_or_insert(JobOutcome::Failed { seq, error });
            if job.plan.kind == OpKind::Rename {
                break;
            }
        }
    }
    first.unwrap_or(JobOutcome::Done)
}

fn is_plain_move(kind: OpKind, step: &Step, held: Option<&HeldFile>) -> bool {
    kind == OpKind::Move && held.is_none() && matches!(step, Step::Rename { .. })
}

fn checked_undo(
    run: &Rollback<'_>,
    (seq, step): (u32, &Step),
    held: Option<&HeldFile>,
    ctx: &mut JobContext<'_>,
) -> Result<StepOutcome, FsError> {
    let unsure = run.job.unsure == Some(seq) && !run.is_done(seq) && held.is_none();
    let verified = if run.job.plan.kind == OpKind::Rename {
        super::rename::verified(seq, step, ctx)?
    } else {
        true
    };
    if !verified {
        Ok(StepOutcome::Skipped)
    } else if unsure {
        undo_unsure(ctx.ops, step)
    } else {
        undo(run, (seq, step), held, ctx)
    }
}

/// The held row for a step's target, if the step writes one.
pub(super) fn held_for<'j>(step: &Step, run: &Rollback<'j>) -> Option<&'j HeldFile> {
    let target = match step {
        Step::CopyFile { target, .. }
        | Step::Rename { target, .. }
        | Step::WriteFile { target, .. } => target,
        _ => return None,
    };
    run.held_for(target)
}

fn awaits_unreadable_row(run: &Rollback<'_>, step: &Step, held: Option<&HeldFile>) -> bool {
    if !run.job.held_unreadable || held.is_some() {
        return false;
    }
    match step {
        Step::CopyFile { .. } => true,
        Step::Rename { target, .. } => run.overwrites(target),
        Step::WriteFile { .. } => true,
        _ => false,
    }
}

fn undo(
    run: &Rollback<'_>,
    (seq, step): (u32, &Step),
    held: Option<&HeldFile>,
    ctx: &mut JobContext<'_>,
) -> Result<StepOutcome, FsError> {
    let ops = ctx.ops;
    match step {
        Step::CopyFile { source, target, .. } => {
            if source_gone(run, source, held, ops)? || only_copy(run, (seq, source, target), ctx)? {
                return held.map_or(Ok(StepOutcome::Skipped), |held| {
                    settle(keep(ops, held)?, held, ctx)
                });
            }
            match held {
                Some(held) => settle(restore(ops, held)?, held, ctx),
                None => remove_copy(ops, target, run.placed_for(target).map(|file| file.stamp)),
            }
        }
        Step::MakeFolder { target } => Ok(match ops.remove_empty_folder(target) {
            Ok(()) => StepOutcome::Done,
            Err(_) => StepOutcome::Skipped,
        }),
        Step::Rename { source, target } => match held {
            Some(held) => undo_move_over(source, target, held, ctx),
            None => undo_rename(ops, source, target),
        },
        Step::MakeLink { at, target_of } => undo_link(ops, at, target_of),
        Step::WriteFile { .. } => super::write_file::recover(ctx, held),
        Step::Recycle { .. }
        | Step::RemoveFile { .. }
        | Step::RemoveLink { .. }
        | Step::RemoveFolder { .. } => Ok(StepOutcome::Skipped),
    }
}

fn undo_rename(
    ops: &dyn ProviderOps,
    source: &Location,
    target: &Location,
) -> Result<StepOutcome, FsError> {
    if surely_absent(ops, target)? && presence(ops, source)?.is_some() {
        return Ok(StepOutcome::Skipped);
    }
    ops.rename(target, source).map(|()| StepOutcome::Done)
}

fn undo_unsure(ops: &dyn ProviderOps, step: &Step) -> Result<StepOutcome, FsError> {
    let (source, target) = match step {
        Step::Rename { source, target } => (source, target),
        Step::MakeLink { at, target_of } => return undo_link(ops, at, target_of),
        _ => return Ok(StepOutcome::Skipped),
    };
    if presence(ops, source)?.is_some() || presence(ops, target)?.is_none() {
        return Ok(StepOutcome::Skipped);
    }
    ops.rename(target, source).map(|()| StepOutcome::Done)
}

fn undo_move_over(
    source: &Location,
    target: &Location,
    held: &HeldFile,
    ctx: &mut JobContext<'_>,
) -> Result<StepOutcome, FsError> {
    let ops = ctx.ops;
    if presence(ops, &held.held)?.is_some() {
        move_back(ops, source, target, held)?;
    }
    settle(keep(ops, held)?, held, ctx)
}

fn move_back(
    ops: &dyn ProviderOps,
    source: &Location,
    target: &Location,
    held: &HeldFile,
) -> Result<(), FsError> {
    let Some(at) = presence(ops, target)? else {
        return Ok(());
    };
    if !is_placed(&at, held) {
        return Ok(());
    }
    match ops.rename(target, source) {
        Err(error) if already_exists(&error) => Ok(()),
        Err(error)
            if error.kind == FsErrorKind::NotFound
                && gone_or_folder_deleted(ops, source, held.volume)? =>
        {
            Ok(())
        }
        other => other,
    }
}

fn remove_copy(
    ops: &dyn ProviderOps,
    target: &Location,
    stamp: Option<FileStamp>,
) -> Result<StepOutcome, FsError> {
    let Some(placed) = presence(ops, target)? else {
        return Ok(StepOutcome::Skipped);
    };
    if !stamp.is_some_and(|stamp| matches_stamp(&placed, stamp)) {
        return Ok(StepOutcome::Skipped);
    }
    match ops.remove_file(target) {
        Ok(()) => Ok(StepOutcome::Done),
        Err(error) if error.kind == FsErrorKind::NotFound => Ok(StepOutcome::Skipped),
        Err(error) => Err(error),
    }
}

fn source_gone(
    run: &Rollback<'_>,
    source: &Location,
    held: Option<&HeldFile>,
    ops: &dyn ProviderOps,
) -> Result<bool, FsError> {
    if run.removal_done(source) {
        return Ok(true);
    }
    gone_or_folder_deleted(ops, source, held.and_then(|file| file.volume))
}

/// True when a done copy of a move may be the only copy left: its source removal may have run
/// before a crash, and the source path does not prove it still holds the file the copy read
/// (another file there, or a journal row without a proof). Such a copy stays and is reported.
fn only_copy(
    run: &Rollback<'_>,
    (seq, source, target): (u32, &Location, &Location),
    ctx: &mut JobContext<'_>,
) -> Result<bool, FsError> {
    if run.job.plan.kind != OpKind::Move || !run.is_done(seq) {
        return Ok(false);
    }
    // The newest row wins: a retried copy journals again, and its proof describes the copy on disk.
    let proof = run.newest_placed(target).and_then(|file| file.source);
    let proved = match proof {
        Some(proof) if proof.identity.is_some() => proof.still_at(ctx.ops, source)?,
        Some(proof) => {
            proof.still_at(ctx.ops, source)?
                && (!removal_may_have_run(run, source) || same_bytes(ctx, source, target))
        }
        None => false,
    };
    if !proved {
        tracing::warn!(
            seq,
            "move rollback kept a copy: its source is not proved to be the original"
        );
        ctx.log.kept_copy(&KeptCopy {
            seq,
            location: target.clone(),
        });
    }
    Ok(!proved)
}

/// True when this job's removal of `source` may have run without its done row (D165): the plan
/// removes `source` within `job.unmarked_removals`. Unknown pending state counts as maybe.
fn removal_may_have_run(run: &Rollback<'_>, source: &Location) -> bool {
    let Some(unmarked) = &run.job.unmarked_removals else {
        return true;
    };
    run.removes_within(source, unmarked)
}

/// The seqs whose removal may have run without a done row (D165), worked out once per rollback
/// from the steps still pending. A move starts a removal only after every earlier copy, folder or
/// rename step wrote its row, so the window ends at the first pending one of those. Pending
/// removals do not end it: a failed removal's row is best effort, and a move goes on past a kept
/// source. Empty when no removal can have started.
pub(super) fn unmarked_removals(
    plan: &OperationPlan,
    pending: impl IntoIterator<Item = u32>,
) -> Range<u32> {
    let removes = |seq: u32| plan.steps.get(seq as usize).map(is_removal);
    let first = (0_u32..)
        .zip(&plan.steps)
        .find(|(_, step)| is_removal(step));
    let unstarted = pending
        .into_iter()
        .filter(|seq| removes(*seq) == Some(false))
        .min();
    first.map_or(u32::MAX, |(seq, _)| seq)..unstarted.unwrap_or(u32::MAX)
}

#[cfg(test)]
thread_local! {
    /// Removal steps the D165 check looked at on this thread, so a test can prove a copy-phase
    /// crash checks each copy without a scan.
    pub(super) static STEPS_SCANNED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// Probes, rows and seqs a rollback compared on this thread, so a test can prove the whole
    /// rollback stays linear in the plan size (MR1f).
    pub(super) static COMPARED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Counts one compare in test builds; does nothing otherwise.
pub(super) fn compared() {
    #[cfg(test)]
    COMPARED.with(|count| count.set(count.get() + 1));
}

/// Counts one scanned step in test builds; does nothing otherwise.
pub(super) fn scanned() {
    #[cfg(test)]
    STEPS_SCANNED.with(|count| count.set(count.get() + 1));
}

/// True when `source` and `copy` hold the same bytes (BLAKE3, streamed). A proof without a file
/// ID can match a lookalike that Windows name tunneling gave the old creation time; any read
/// error or cancel counts as different, so the copy stays.
fn same_bytes(ctx: &JobContext<'_>, source: &Location, copy: &Location) -> bool {
    let ops = ctx.ops;
    let compared = ops
        .content_hash(source, ctx.cancel)
        .and_then(|left| Ok(left == ops.content_hash(copy, ctx.cancel)?));
    compared.unwrap_or_else(|error| {
        tracing::warn!(kind = ?error.kind, "move rollback could not compare a copy with its source");
        false
    })
}

pub(super) fn settle(
    restored: Restored,
    held: &HeldFile,
    ctx: &mut JobContext<'_>,
) -> Result<StepOutcome, FsError> {
    Ok(match restored {
        Restored::Back => StepOutcome::Done,
        Restored::Beside(beside) => {
            ctx.log.kept_beside(held, &beside);
            StepOutcome::Done
        }
        Restored::Gone => StepOutcome::Skipped,
    })
}

pub(super) fn clean_up(
    plan: &OperationPlan,
    done: &[u32],
    ops: &dyn ProviderOps,
) -> Result<u32, FsError> {
    let done = super::done_set::DoneSet::of_plan(plan, done);
    let mut removed = 0;
    for (seq, step) in (0_u32..).zip(&plan.steps) {
        if done.contains(&seq) {
            continue;
        }
        if let Step::CopyFile { target, .. } = step {
            for temp in [temp_of(target)?, legacy_temp_of(target)?] {
                removed += u32::from(remove_if_present(ops, &temp)?);
            }
        }
    }
    Ok(removed)
}
