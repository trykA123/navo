use rusty_core::{FsError, FsErrorKind, Location, OpKind, OperationPlan, Step};

use super::aside::Aside;
use super::copy::{Copier, CopyStep, copy, overwrites, place};
use super::hold::hold;
use super::marks::Marks;
use super::remove::{Removals, remove};
use super::rename::{is_temporary, prepare};
use super::{JobContext, JobOutcome, Progress, StepOutcome};

pub(super) fn current_of(step: &Step) -> Location {
    match step {
        Step::MakeFolder { target } => target.clone(),
        Step::MakeLink { at, .. } => at.clone(),
        Step::CopyFile { source, .. } | Step::Rename { source, .. } => source.clone(),
        Step::Recycle { location }
        | Step::RemoveFile { location }
        | Step::RemoveLink { location }
        | Step::RemoveFolder { location } => location.clone(),
    }
}

pub(super) fn counted(kind: OpKind, step: &Step) -> (u64, u64) {
    match step {
        Step::CopyFile { size, .. } => (*size, 1),
        Step::RemoveFile { .. } | Step::MakeLink { .. } => (0, 1),
        Step::Rename { target, .. } if kind == OpKind::Rename && !is_temporary(target) => (0, 1),
        _ => (0, 0),
    }
}

pub(super) fn moved_bytes(plan: &OperationPlan) -> u64 {
    plan.steps
        .iter()
        .map(|step| counted(plan.kind, step).0)
        .sum()
}

#[derive(Clone, Copy)]
pub(super) struct Job<'p> {
    pub plan: &'p OperationPlan,
    pub aside: &'p Aside,
    pub namespace: Option<&'p str>,
    pub removals: &'p Removals<'p>,
}

pub(super) fn run(plan: &OperationPlan, done: &[u32], ctx: &mut JobContext<'_>) -> JobOutcome {
    let aside = Aside::new();
    let removals = Removals::of(plan);
    let namespace = if ctx.log.preserve_overwrites() {
        ctx.log.temporary_namespace()
    } else {
        None
    };
    let job = Job {
        plan,
        aside: &aside,
        namespace: namespace.as_deref(),
        removals: &removals,
    };
    walk(job, done, ctx)
}

fn walk(job: Job<'_>, done: &[u32], ctx: &mut JobContext<'_>) -> JobOutcome {
    let mut marks = Marks::new(job.plan.kind);
    let outcome = walk_marking((job, done), ctx, &mut marks);
    match marks.flush(ctx.log) {
        Err((seq, error)) if !matches!(outcome, JobOutcome::Failed { .. }) => {
            JobOutcome::Failed { seq, error }
        }
        _ => outcome,
    }
}

fn walk_marking(
    (job, done): (Job<'_>, &[u32]),
    ctx: &mut JobContext<'_>,
    marks: &mut Marks,
) -> JobOutcome {
    let plan = job.plan;
    let mut progress = Progress::start(plan);
    let mut kept = Vec::new();
    for (seq, step) in (0_u32..).zip(&plan.steps) {
        if done.contains(&seq) {
            advance(&mut progress, plan.kind, step);
            continue;
        }
        if ctx.cancel.is_paused()
            && let Err((seq, error)) = marks.flush(ctx.log)
        {
            return JobOutcome::Failed { seq, error };
        }
        hold(ctx.cancel, &mut progress, ctx.progress);
        if ctx.cancel.is_cancelled() {
            return JobOutcome::Cancelled;
        }
        progress.current = Some(current_of(step));
        let start = progress.bytes_done;
        let result = if holds_kept(step, &kept) {
            Ok(StepOutcome::Skipped)
        } else {
            prepare(seq, (plan.kind, step), ctx)
                .and_then(|()| apply(job, (seq, step), ctx, &mut progress))
        };
        match result {
            Ok(outcome) => {
                note_kept(step, outcome, &mut kept);
                if let Err((seq, error)) = marks.record(ctx.log, (seq, step), outcome) {
                    return JobOutcome::Failed { seq, error };
                }
                progress.bytes_done = start;
                advance(&mut progress, plan.kind, step);
                (ctx.progress)(&progress);
            }
            Err(error) if error.kind == FsErrorKind::Cancelled || ctx.cancel.is_cancelled() => {
                return JobOutcome::Cancelled;
            }
            Err(error) => {
                marks.failed(ctx.log, seq);
                return JobOutcome::Failed { seq, error };
            }
        }
    }
    JobOutcome::Done
}

pub(super) fn note_kept(step: &Step, outcome: StepOutcome, kept: &mut Vec<Location>) {
    if let Step::CopyFile { source, .. } = step
        && outcome != StepOutcome::Done
    {
        kept.push(source.clone());
    }
}

pub(super) fn holds_kept(step: &Step, kept: &[Location]) -> bool {
    let location = match step {
        Step::RemoveFile { location }
        | Step::RemoveLink { location }
        | Step::RemoveFolder { location } => location,
        _ => return false,
    };
    kept.iter().any(|source| {
        std::iter::successors(Some(source.clone()), Location::parent)
            .any(|ancestor| ancestor.same_place(location))
    })
}

fn advance(progress: &mut Progress, kind: OpKind, step: &Step) {
    let (bytes, files) = counted(kind, step);
    progress.bytes_done += bytes;
    progress.files_done += files;
}

pub(super) fn apply(
    job: Job<'_>,
    (seq, step): (u32, &Step),
    ctx: &mut JobContext<'_>,
    progress: &mut Progress,
) -> Result<StepOutcome, FsError> {
    let ops = ctx.ops;
    let done = |result: Result<(), FsError>| result.map(|()| StepOutcome::Done);
    match step {
        Step::MakeFolder { target } => make_folder(ctx, target),
        Step::CopyFile {
            source,
            target,
            size,
        } => {
            let step = CopyStep {
                seq,
                source,
                target,
                size: *size,
            };
            copy_in_order(job, &step, ctx, progress)
        }
        Step::Rename { source, target } => done(rename_step(job, ctx, source, target)),
        Step::Recycle { location } => done(ops.recycle(location)),
        Step::RemoveFile { .. } | Step::RemoveLink { .. } | Step::RemoveFolder { .. } => {
            remove(ops, job.removals, step)
        }
        Step::MakeLink { target_of, at } => done(ops.make_link(target_of, at)),
    }
}

fn rename_step(
    job: Job<'_>,
    ctx: &mut JobContext<'_>,
    source: &Location,
    target: &Location,
) -> Result<(), FsError> {
    if overwrites(job.plan, target) {
        return job.aside.move_over(ctx, source, target);
    }
    if ctx.log.preserve_overwrites() {
        ctx.ops.rename_atomic(source, target)
    } else {
        ctx.ops.rename(source, target)
    }
}

fn copy_in_order(
    job: Job<'_>,
    step: &CopyStep<'_>,
    ctx: &mut JobContext<'_>,
    progress: &mut Progress,
) -> Result<StepOutcome, FsError> {
    super::temporary::prepare(ctx, step, job.namespace)?;
    let copier = Copier {
        plan: job.plan,
        ops: ctx.ops,
        cancel: ctx.cancel,
        strict: ctx.log.preserve_overwrites(),
        namespace: job.namespace,
        removals: job.removals,
    };
    let base = progress.bytes_done;
    let (cancel, report) = (ctx.cancel, &mut *ctx.progress);
    let mut on_bytes = |bytes: u64| {
        progress.bytes_done = base + bytes;
        report(progress);
        hold(cancel, progress, report);
    };
    let mut created = |identity| ctx.log.created_temporary((step.seq, identity), ctx.ops);
    let copied = copy(
        &copier,
        step,
        super::copy::CopyHooks {
            progress: &mut on_bytes,
            created: &mut created,
        },
    )?;
    place(job.aside, ctx, copied, step.target)
}

fn make_folder(ctx: &mut JobContext<'_>, target: &Location) -> Result<StepOutcome, FsError> {
    let existing = ctx.ops.metadata(target);
    if existing.is_ok_and(|entry| entry.kind == crate::EntryKind::Folder) {
        return Ok(StepOutcome::Skipped);
    }
    ctx.ops.create_folder(target).map(|()| StepOutcome::Done)
}
