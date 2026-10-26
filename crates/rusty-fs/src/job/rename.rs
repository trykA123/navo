use std::collections::{HashMap, HashSet};

use rusty_core::{FsError, FsErrorKind, Location, OpKind, OperationPlan, Step};

use crate::FileIdentity;

use super::aside::presence;
use super::{
    HeldFile, JobContext, JobLog, JobOutcome, Journaled, PlacedFile, StepOutcome, roll_back, run,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameOutcome {
    Done,
    PutBack {
        seq: u32,
        error: FsError,
    },
    Cancelled,
    Stuck {
        error: FsError,
        left: Vec<Location>,
        applied: Vec<u32>,
    },
}

pub fn is_temporary(target: &Location) -> bool {
    target
        .name()
        .is_some_and(|name| rusty_core::brand::is_rename_temp(&name.to_text()))
}

struct Tracking<'a> {
    inner: &'a mut dyn JobLog,
    done: Vec<u32>,
    failed: Vec<u32>,
    undone: Vec<u32>,
    identities: HashMap<u32, FileIdentity>,
}

impl<'a> Tracking<'a> {
    fn new(inner: &'a mut dyn JobLog) -> Self {
        Self {
            inner,
            done: Vec::new(),
            failed: Vec::new(),
            undone: Vec::new(),
            identities: HashMap::new(),
        }
    }
}

impl JobLog for Tracking<'_> {
    fn prepare_rename(&mut self, seq: u32, identity: FileIdentity) -> Result<(), FsError> {
        self.inner.prepare_rename(seq, identity)?;
        self.identities.insert(seq, identity);
        Ok(())
    }

    fn rename_identity(&self, seq: u32) -> Result<Option<FileIdentity>, FsError> {
        match self.identities.get(&seq) {
            Some(identity) => Ok(Some(*identity)),
            None => self.inner.rename_identity(seq),
        }
    }

    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        match outcome {
            StepOutcome::Done => self.done.push(seq),
            StepOutcome::Failed => self.failed.push(seq),
            StepOutcome::Skipped => {}
        }
        self.inner.step(seq, outcome)
    }

    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        if outcome == StepOutcome::Failed {
            self.failed.push(seq);
        } else {
            self.undone.push(seq);
        }
        self.inner.undone(seq, outcome)
    }

    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError> {
        self.inner.journal_held(held)
    }

    fn journal_placed(&mut self, placed: &PlacedFile) -> Result<(), FsError> {
        self.inner.journal_placed(placed)
    }

    fn kept_beside(&mut self, held: &HeldFile, beside: &Location) {
        self.inner.kept_beside(held, beside);
    }

    fn held_stranded(&mut self, held: &HeldFile) {
        self.inner.held_stranded(held);
    }
}

#[must_use]
pub(super) fn run_rename(plan: &OperationPlan, ctx: &mut JobContext<'_>) -> RenameOutcome {
    let JobContext {
        ops,
        log,
        cancel,
        progress,
    } = ctx;
    let mut tracking = Tracking::new(&mut **log);
    let mut inner = JobContext {
        ops: *ops,
        log: &mut tracking,
        cancel,
        progress: &mut **progress,
    };
    let first = match run::run(plan, &[], &mut inner) {
        JobOutcome::Done => return RenameOutcome::Done,
        JobOutcome::Cancelled => None,
        JobOutcome::Failed { seq, error } => Some((seq, error)),
    };
    let done = std::mem::take(&mut tracking.done);
    tracking.failed.clear();
    let job = Journaled {
        plan,
        done: &done,
        held: &[],
        placed: &[],
        held_unreadable: false,
        unsure: first.as_ref().map(|(seq, _)| *seq),
    };
    let mut inner = JobContext {
        ops: *ops,
        log: &mut tracking,
        cancel,
        progress: &mut **progress,
    };
    match roll_back(&job, &mut inner) {
        JobOutcome::Failed { error, .. } => stuck(
            plan,
            (*ops, &tracking),
            (error, &done, first.as_ref().map(|(seq, _)| *seq)),
        ),
        JobOutcome::Done | JobOutcome::Cancelled => match first {
            Some((seq, error)) => RenameOutcome::PutBack { seq, error },
            None => RenameOutcome::Cancelled,
        },
    }
}

pub(super) fn prepare(
    seq: u32,
    step: (OpKind, &Step),
    ctx: &mut JobContext<'_>,
) -> Result<(), FsError> {
    let (OpKind::Rename, Step::Rename { source, .. }) = step else {
        return Ok(());
    };
    let identity = match ctx.ops.file_identity(source)? {
        Some(identity) => identity,
        None => {
            let entry = ctx.ops.metadata(source)?;
            FileIdentity::fingerprint(
                entry.size,
                entry.modified_ms,
                entry.kind == crate::EntryKind::Folder,
            )
        }
    };
    ctx.log.prepare_rename(seq, identity)
}

pub(super) fn verified(seq: u32, step: &Step, ctx: &JobContext<'_>) -> Result<bool, FsError> {
    let Step::Rename { source, target } = step else {
        return Ok(false);
    };
    let identity = ctx.log.rename_identity(seq)?;
    if identity.is_none()
        && presence(ctx.ops, source)?.is_none()
        && presence(ctx.ops, target)?.is_some()
    {
        return Err(FsError {
            location: Some(target.clone()),
            ..FsError::new(FsErrorKind::IdentityUnavailable)
        });
    }
    if ctx.ops.matches_identity(target, identity)? {
        return Ok(true);
    }
    if identity.is_some() && !ctx.ops.matches_identity(source, identity)? {
        return Err(FsError {
            location: Some(target.clone()),
            ..FsError::new(FsErrorKind::IdentityUnavailable)
        });
    }
    Ok(false)
}

fn left_over(plan: &OperationPlan, failed: &[u32], undone: &[u32], done: &[u32]) -> Vec<Location> {
    let not_run = done.iter().filter(|seq| !undone.contains(seq));
    let mut seqs: Vec<u32> = failed.iter().chain(not_run).copied().collect();
    seqs.sort_unstable();
    seqs.dedup();
    seqs.iter()
        .filter_map(|&seq| match plan.steps.get(usize::try_from(seq).ok()?)? {
            Step::Rename { target, .. } => Some(target.clone()),
            _ => None,
        })
        .collect()
}

fn remaining(done: &[u32], undone: &[u32]) -> Vec<u32> {
    let undone: HashSet<_> = undone.iter().copied().collect();
    done.iter()
        .copied()
        .filter(|seq| !undone.contains(seq))
        .collect()
}

fn stuck(
    plan: &OperationPlan,
    (ops, tracking): (&dyn crate::ProviderOps, &Tracking<'_>),
    (mut error, done, unsure): (FsError, &[u32], Option<u32>),
) -> RenameOutcome {
    let mut applied = remaining(done, &tracking.undone);
    match confirm_uncertain(plan, (ops, tracking), (done, unsure)) {
        Ok(Some(seq)) => applied.push(seq),
        Ok(None) => {}
        Err(cause) => error = cause,
    }
    applied.sort_unstable();
    RenameOutcome::Stuck {
        error,
        left: left_over(plan, &tracking.failed, &tracking.undone, done),
        applied,
    }
}

fn confirm_uncertain(
    plan: &OperationPlan,
    (ops, tracking): (&dyn crate::ProviderOps, &Tracking<'_>),
    (done, unsure): (&[u32], Option<u32>),
) -> Result<Option<u32>, FsError> {
    let Some(seq) = unsure.filter(|seq| !done.contains(seq) && !tracking.undone.contains(seq))
    else {
        return Ok(None);
    };
    let step = usize::try_from(seq).ok().and_then(|at| plan.steps.get(at));
    let Some(Step::Rename { target, .. }) = step else {
        return Ok(None);
    };
    Ok(ops
        .matches_identity(target, tracking.rename_identity(seq)?)?
        .then_some(seq))
}
