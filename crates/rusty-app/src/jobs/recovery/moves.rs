use rusty_core::{FsError, FsErrorKind, Location, OpKind, OperationPlan, Step};
use rusty_fs::{HeldFile, JobLog, PreparedMove, ProviderOps, recorded_proof, settle_prepared_move};
use rusty_store::{JournalStep, StepState, UnfinishedJob};

#[derive(Default)]
pub(super) struct Settled {
    pub(super) completed: Vec<u32>,
    pub(super) unresolved: usize,
}

pub(super) fn has_plain_moves(plan: &OperationPlan) -> bool {
    plan.kind == OpKind::Move
        && plan
            .steps
            .iter()
            .any(|step| matches!(step, Step::Rename { .. }))
}

pub(super) fn plain_moves(plan: &OperationPlan, held: &[HeldFile]) -> Vec<u32> {
    if plan.kind != OpKind::Move {
        return Vec::new();
    }
    (0_u32..)
        .zip(&plan.steps)
        .filter_map(|(seq, step)| match step {
            Step::Rename { target, .. } if !is_held(held, target) => Some(seq),
            _ => None,
        })
        .collect()
}

fn is_held(held: &[HeldFile], target: &Location) -> bool {
    held.iter().any(|file| file.original.same_place(target))
}

pub(super) fn settle_pending(
    (plan, job): (&OperationPlan, &UnfinishedJob),
    held: &[HeldFile],
    (log, ops): (&dyn JobLog, &dyn ProviderOps),
) -> Settled {
    let mut settled = Settled::default();
    if plan.kind != OpKind::Move {
        return settled;
    }
    for step in job.steps.iter().filter(|step| is_pending(step)) {
        let planned = usize::try_from(step.seq)
            .ok()
            .and_then(|at| plan.steps.get(at));
        let Some(planned @ Step::Rename { target, .. }) = planned else {
            continue;
        };
        if is_held(held, target) {
            continue;
        }
        let settled_step = recorded_proof(log, step.seq).and_then(|proof| match proof {
            Some(proof) => settle_prepared_move(ops, planned, &proof),
            None => unproved(ops, target),
        });
        match settled_step {
            Ok(PreparedMove::Completed) => settled.completed.push(step.seq),
            Ok(PreparedMove::NotStarted) => {}
            Ok(PreparedMove::Unresolved) | Err(_) => {
                tracing::warn!(
                    job = job.id.0,
                    seq = step.seq,
                    "an unfinished move step is left in place"
                );
                settled.unresolved += 1;
            }
        }
    }
    settled
}

fn unproved(ops: &dyn ProviderOps, target: &Location) -> Result<PreparedMove, FsError> {
    match ops.metadata(target) {
        Ok(_) => Ok(PreparedMove::Unresolved),
        Err(error) if error.kind == FsErrorKind::NotFound => Ok(PreparedMove::NotStarted),
        Err(error) => Err(error),
    }
}

fn is_pending(step: &JournalStep) -> bool {
    step.state == StepState::Pending && !step.is_undone()
}
