use rusty_core::{FsError, Location, OperationPlan, Step};

use super::{
    HeldFile, JobContext, JobLog, JobOutcome, Journaled, PlacedFile, StepOutcome, roll_back, run,
};

pub(super) const TEMP_PREFIX: &str = ".ferrite-rename-";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameOutcome {
    Done,
    PutBack { seq: u32, error: FsError },
    Cancelled,
    Stuck { error: FsError, left: Vec<Location> },
}

pub(super) fn is_temporary(target: &Location) -> bool {
    target
        .name()
        .is_some_and(|name| name.to_text().starts_with(TEMP_PREFIX))
}

struct Tracking<'a> {
    inner: &'a mut dyn JobLog,
    done: Vec<u32>,
    failed: Vec<u32>,
}

impl JobLog for Tracking<'_> {
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
    let mut tracking = Tracking {
        inner: &mut **log,
        done: Vec::new(),
        failed: Vec::new(),
    };
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
        JobOutcome::Failed { error, .. } => RenameOutcome::Stuck {
            error,
            left: left_over(plan, &tracking.failed),
        },
        JobOutcome::Done | JobOutcome::Cancelled => match first {
            Some((seq, error)) => RenameOutcome::PutBack { seq, error },
            None => RenameOutcome::Cancelled,
        },
    }
}

fn left_over(plan: &OperationPlan, failed: &[u32]) -> Vec<Location> {
    let mut seqs = failed.to_vec();
    seqs.sort_unstable();
    seqs.dedup();
    seqs.iter()
        .filter_map(|&seq| match plan.steps.get(usize::try_from(seq).ok()?)? {
            Step::Rename { target, .. } => Some(target.clone()),
            _ => None,
        })
        .collect()
}
