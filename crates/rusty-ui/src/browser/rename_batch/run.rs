use std::collections::HashSet;

use rusty_core::{FsError, FsErrorKind, Location, OperationPlan, Step};
use rusty_fs::{
    HeldFile, JobContext, JobLog, JobOutcome, Progress, ProviderOps, RenameOutcome, StepOutcome,
    is_rename_temporary, run_rename,
};

use super::hooks::{RenameReport, is_item};

pub(in crate::browser) fn run_batch(
    plan: &OperationPlan,
    ctx: &mut JobContext<'_>,
) -> (JobOutcome, RenameReport) {
    match run_rename(plan, ctx) {
        RenameOutcome::Done => (JobOutcome::Done, RenameReport::default()),
        RenameOutcome::Cancelled => (JobOutcome::Cancelled, RenameReport::default()),
        RenameOutcome::PutBack { seq, error } => {
            (JobOutcome::Failed { seq, error }, RenameReport::default())
        }
        RenameOutcome::Stuck {
            error,
            left,
            applied,
        } => {
            let seq = left.first().map_or(0, |first| seq_of(plan, first));
            let report = RenameReport {
                left,
                applied,
                skipped: 0,
                ..RenameReport::default()
            };
            (JobOutcome::Failed { seq, error }, report)
        }
    }
}

fn seq_of(plan: &OperationPlan, target: &Location) -> u32 {
    (0_u32..)
        .zip(&plan.steps)
        .find(|(_, step)| matches!(step, Step::Rename { target: at, .. } if at == target))
        .map_or(0, |(seq, _)| seq)
}

struct Counting<'a> {
    inner: &'a mut dyn JobLog,
    progress: &'a mut dyn FnMut(&Progress),
    state: Progress,
    plan: &'a OperationPlan,
}

impl JobLog for Counting<'_> {
    fn rename_identity(&self, seq: u32) -> Result<Option<rusty_fs::FileIdentity>, FsError> {
        self.inner.rename_identity(seq)
    }

    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.inner.step(seq, outcome)
    }

    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        let result = self.inner.undone(seq, outcome);
        let step = usize::try_from(seq)
            .ok()
            .and_then(|at| self.plan.steps.get(at));
        if outcome == StepOutcome::Done
            && let Some(step) = step.filter(|step| is_item(step))
        {
            self.state.files_done += 1;
            if let Step::Rename { target, .. } = step {
                self.state.current = Some(target.clone());
            }
            (self.progress)(&self.state);
        }
        result
    }

    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError> {
        self.inner.journal_held(held)
    }
}

pub(in crate::browser) fn run_undo(
    plan: &OperationPlan,
    done: &[u32],
    ctx: &mut JobContext<'_>,
) -> (JobOutcome, RenameReport) {
    let JobContext {
        ops, log, progress, ..
    } = ctx;
    let ops: &dyn ProviderOps = *ops;
    let mut counting = Counting {
        inner: &mut **log,
        progress: &mut **progress,
        state: Progress {
            bytes_done: 0,
            bytes_total: 0,
            files_done: 0,
            files_total: plan.totals.files,
            current: None,
            paused: false,
        },
        plan,
    };
    let done: HashSet<u32> = done.iter().copied().collect();
    let mut skipped = 0;
    let mut first = None;
    let steps: Vec<(u32, &Step)> = (0_u32..).zip(&plan.steps).collect();
    for &(seq, step) in steps.iter().rev() {
        let Step::Rename { source, target } = step else {
            continue;
        };
        if !done.contains(&seq) {
            continue;
        }
        let result = match still_undoable(ops, &counting, (seq, source, target)) {
            Ok(true) => ops.rename(target, source).map(|()| StepOutcome::Done),
            Ok(false) => {
                skipped += usize::from(is_item(step));
                Ok(StepOutcome::Skipped)
            }
            Err(error) => Err(error),
        };
        first = logged(seq, result, &mut counting);
        if first.is_some() {
            break;
        }
    }
    let (left, unsure) = left_under_temporary_names(plan, ops);
    let report = RenameReport {
        left,
        skipped,
        ..RenameReport::default()
    };
    let outcome = first.or(unsure).unwrap_or(JobOutcome::Done);
    (outcome, report)
}

fn logged(
    seq: u32,
    result: Result<StepOutcome, FsError>,
    log: &mut dyn JobLog,
) -> Option<JobOutcome> {
    let written = match result {
        Ok(outcome) => log.undone(seq, outcome),
        Err(error) => {
            let _ = log.undone(seq, StepOutcome::Failed);
            Err(error)
        }
    };
    written.err().map(|error| JobOutcome::Failed { seq, error })
}

fn is_there(ops: &dyn ProviderOps, location: &Location) -> Result<bool, FsError> {
    match ops.metadata(location) {
        Ok(_) => Ok(true),
        Err(error) if error.kind == FsErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn still_undoable(
    ops: &dyn ProviderOps,
    log: &Counting<'_>,
    (seq, source, target): (u32, &Location, &Location),
) -> Result<bool, FsError> {
    if !is_there(ops, target)? {
        return Ok(false);
    }
    let identity = log.rename_identity(seq)?;
    let matches = ops.matches_identity(target, identity)?;
    if !matches
        && identity
            .is_some_and(|identity| identity.kind == rusty_fs::FileIdentityKind::WindowsIndex)
    {
        return Err(FsError {
            location: Some(target.clone()),
            ..FsError::new(FsErrorKind::IdentityUnavailable)
        });
    }
    if is_there(ops, source)?
        && !log
            .plan
            .steps
            .iter()
            .any(|step| matches!(step, Step::Rename { target, .. } if target.same_place(source)))
    {
        return Ok(false);
    }
    Ok(matches)
}

fn left_under_temporary_names(
    plan: &OperationPlan,
    ops: &dyn ProviderOps,
) -> (Vec<Location>, Option<JobOutcome>) {
    let mut left = Vec::new();
    let mut unsure = None;
    for target in plan.steps.iter().filter_map(|step| match step {
        Step::Rename { target, .. } if is_rename_temporary(target) => Some(target),
        _ => None,
    }) {
        match is_there(ops, target) {
            Ok(false) => continue,
            Ok(true) => {}
            Err(error) => {
                let seq = seq_of(plan, target);
                unsure.get_or_insert(JobOutcome::Failed { seq, error });
            }
        }
        left.push(target.clone());
    }
    (left, unsure)
}
