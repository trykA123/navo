use std::sync::Arc;

use navo_core::retention::{RetainedState, RetentionId};
use navo_core::{FsError, FsErrorKind, Location, OpKind, OperationPlan, Step};
use navo_fs::{
    CancelToken, HeldFile, JobContext, JobLog, JobOutcome, Journaled, ProviderOps, StepOutcome,
};
use navo_store::{JobId, JobState, RetentionCatalog, StepState, StoreDirs};

use super::super::{SharedJournal, held, lock};
use super::{PreviousVersion, RestoreError};

struct UndoLog<'a> {
    journal: &'a SharedJournal,
    job: JobId,
    beside: Option<Location>,
    restoring: Option<HeldFile>,
    done: usize,
    stranded: bool,
}

impl JobLog for UndoLog<'_> {
    fn step(&mut self, _: u32, _: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        if outcome != StepOutcome::Done {
            return Ok(());
        }
        self.done += 1;
        lock(self.journal)
            .mark_undone(self.job, seq)
            .map_err(|_| FsError::new(FsErrorKind::Unknown))
    }

    fn journal_held(&mut self, file: &HeldFile) -> Result<bool, FsError> {
        held::record_restoring(self.journal, self.job, file)?;
        self.restoring = Some(file.clone());
        Ok(true)
    }

    fn held_stranded(&mut self, _: &HeldFile) {
        self.stranded = true;
    }

    fn kept_beside(&mut self, _: &HeldFile, beside: &Location) {
        self.beside = Some(beside.clone());
    }
}

pub(super) fn undo_held(
    journal: &SharedJournal,
    job: JobId,
    ops: &dyn ProviderOps,
) -> Result<PreviousVersion, RestoreError> {
    let undo = match held_undo(journal, job) {
        Ok(undo) => undo,
        Err(error) => {
            set_state(journal, job, JobState::Done)?;
            return Err(error);
        }
    };
    let mut log = UndoLog {
        journal,
        job,
        beside: None,
        restoring: None,
        done: 0,
        stranded: false,
    };
    let outcome = undo.run(&mut JobContext {
        ops,
        log: &mut log,
        cancel: &CancelToken::new(),
        progress: &mut |_| {},
    });
    let outcome = outcome.and_then(|outcome| {
        if log.done == 0 {
            Err(FsError::new(FsErrorKind::NotFound))
        } else {
            Ok(outcome)
        }
    });
    match outcome {
        Ok(StepOutcome::Done) => {
            set_state(journal, job, JobState::Undone)?;
            held::release_job(journal, job, ops).map_err(failed)?;
            Ok(PreviousVersion::Undone {
                at: log.beside.unwrap_or(undo.old.original),
            })
        }
        other => {
            if log.refused_cleanly(&undo.old, ops) {
                held::settle_restore(journal, job).map_err(failed)?;
            }
            Err(match other {
                Err(error) if error.kind == FsErrorKind::ChangedOnDisk => RestoreError::Changed,
                Err(error) => RestoreError::Fs(error.kind),
                Ok(_) => RestoreError::NotKept,
            })
        }
    }
}

impl UndoLog<'_> {
    fn refused_cleanly(&self, old: &HeldFile, ops: &dyn ProviderOps) -> bool {
        if self.done > 0 || self.stranded || self.beside.is_some() {
            return false;
        }
        let Some(current) = &self.restoring else {
            return true;
        };
        ops.metadata(&current.held)
            .is_err_and(|error| error.kind == FsErrorKind::NotFound)
            && ops.metadata(&current.original).is_ok()
            && ops.metadata(&old.held).is_ok()
    }
}

pub(super) fn claim(journal: &SharedJournal, job: JobId) -> Result<(), RestoreError> {
    let mut journal = lock(journal);
    if journal.read_job(job).map_err(failed)?.0 != JobState::Done {
        return Err(RestoreError::NotKept);
    }
    journal
        .finish(job, JobState::RestorePending)
        .map_err(failed)
}

struct HeldUndo {
    plan: OperationPlan,
    done: Vec<u32>,
    held: held::Held,
    old: HeldFile,
}

impl HeldUndo {
    fn run(&self, ctx: &mut JobContext<'_>) -> Result<StepOutcome, FsError> {
        if self.plan.kind == OpKind::Save {
            let outcome = navo_fs::undo_write(&self.old, ctx)?;
            ctx.log.undone(0, outcome)?;
            return Ok(outcome);
        }
        let mut remaining = self.done.clone();
        let steps: Vec<_> = (0_u32..).zip(&self.plan.steps).collect();
        for (seq, step) in steps.into_iter().rev() {
            if !remaining.contains(&seq) {
                continue;
            }
            let Step::CopyFile { target, .. } = step else {
                continue;
            };
            let Some(old) = self
                .held
                .files
                .iter()
                .find(|file| file.original.same_place(target))
            else {
                continue;
            };
            let outcome = navo_fs::undo_write(old, ctx)?;
            ctx.log.undone(seq, outcome)?;
            remaining.retain(|done| *done != seq);
        }
        let outcome = navo_fs::roll_back(
            &Journaled {
                plan: &self.plan,
                done: &remaining,
                held: &[],
                placed: &self.held.placed,
                held_unreadable: false,
                unsure: None,
            },
            ctx,
        );
        match outcome {
            JobOutcome::Done => Ok(StepOutcome::Done),
            JobOutcome::Failed { error, .. } => Err(error),
            JobOutcome::Cancelled => Err(FsError::new(FsErrorKind::Cancelled)),
        }
    }
}

fn held_undo(journal: &SharedJournal, job: JobId) -> Result<HeldUndo, RestoreError> {
    let (state, stored) = lock(journal).read_job(job).map_err(failed)?;
    let plan = serde_json::from_slice::<OperationPlan>(&stored.plan).map_err(failed)?;
    if state != JobState::RestorePending {
        return Err(RestoreError::NotKept);
    }
    let rows = held::held_of(journal, job).map_err(failed)?;
    if rows.unreadable > 0 {
        return Err(RestoreError::NotKept);
    }
    let old = rows
        .files
        .iter()
        .find(|file| matches_old(&plan, file))
        .cloned()
        .ok_or(RestoreError::NotKept)?;
    let done = stored
        .steps
        .iter()
        .filter(|step| step.state == StepState::Done && !step.is_undone())
        .map(|step| step.seq)
        .collect();
    Ok(HeldUndo {
        plan,
        done,
        held: rows,
        old,
    })
}

fn matches_old(plan: &OperationPlan, file: &HeldFile) -> bool {
    match plan.kind {
        OpKind::Save => matches!(
            plan.steps.as_slice(),
            [Step::WriteFile { target, .. }]
                if file.placed_hash.is_some() && file.original == *target
        ),
        OpKind::Copy => plan.steps.iter().any(|step| {
            matches!(step, Step::CopyFile { target, .. } if file.original.same_place(target))
        }),
        _ => false,
    }
}

fn set_state(journal: &SharedJournal, job: JobId, state: JobState) -> Result<(), RestoreError> {
    lock(journal).finish(job, state).map_err(failed)
}

pub(super) fn retained(
    dirs: &StoreDirs,
    job: JobId,
    ops: &Arc<dyn ProviderOps>,
) -> Result<PreviousVersion, RestoreError> {
    let ids = ready_entries(dirs, job)?;
    if ids.is_empty() {
        return Err(RestoreError::NotKept);
    }
    let hooks = crate::retained_hooks::hooks(Some(dirs), true, Arc::clone(ops));
    let at = ids
        .into_iter()
        .map(|id| hooks.restore(id))
        .collect::<Result<Vec<_>, _>>()
        .map_err(RestoreError::Failed)?;
    Ok(PreviousVersion::Retained { at })
}

fn ready_entries(dirs: &StoreDirs, job: JobId) -> Result<Vec<RetentionId>, RestoreError> {
    let (db, _) = crate::crash::open_db(&dirs.journal_file()).map_err(failed)?;
    let catalog = RetentionCatalog::new(db).map_err(failed)?;
    let mut ready = Vec::new();
    for id in catalog.ids_for_job(job).map_err(failed)? {
        if catalog.get(id).map_err(failed)?.state == RetainedState::Ready {
            ready.push(id);
        }
    }
    Ok(ready)
}

fn failed(error: impl std::fmt::Display) -> RestoreError {
    RestoreError::Failed(error.to_string())
}
