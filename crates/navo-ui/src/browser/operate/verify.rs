//! Verify-after-copy choice in the browser: resolved when a plan is accepted, toggled by the
//! user while it is pending, and handed to the job, whose plain run lives here too (AD10b).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use navo_core::{Location, OpKind, Step};
use navo_fs::{
    DriveClass, JobContext, JobOptions, JobOutcome, VerifyCount, VerifyMode, run_job_with,
};

use super::work::{JobWork, worker_count};
use super::{Browser, ClassOf, PendingOp};

/// The verify setting, the counter of each job (pending or running) that verifies, and the
/// pending jobs the user turned verify off for.
#[derive(Default)]
pub(in crate::browser) struct Verifies {
    mode: VerifyMode,
    by_job: HashMap<u64, Arc<VerifyCount>>,
    declined: HashSet<u64>,
}

/// Says whether `pending` may verify: a copy or move with at least one file copy, never an undo.
fn eligible(pending: &PendingOp) -> bool {
    matches!(pending.plan.kind, OpKind::Copy | OpKind::Move)
        && pending.undo.is_none()
        && pending
            .plan
            .steps
            .iter()
            .any(|step| matches!(step, Step::CopyFile { .. }))
}

/// Drive class of `target` by `class_of`; no target is `Unknown`. Runs on the planning thread.
pub(super) fn target_class(class_of: ClassOf, target: Option<&Location>) -> DriveClass {
    target.map_or(DriveClass::Unknown, class_of)
}

impl Browser {
    /// Sets when new plans verify; plans already accepted keep their choice.
    pub fn set_verify_mode(&mut self, mode: VerifyMode) {
        self.job_queue.verifies.mode = mode;
    }

    #[must_use]
    pub fn verify_mode(&self) -> VerifyMode {
        self.job_queue.verifies.mode
    }

    /// True when the pending plan may verify, so the dialog offers the toggle.
    #[must_use]
    pub fn can_verify(&self) -> bool {
        self.pending.as_ref().is_some_and(eligible)
    }

    /// The verify counter of job `id`, or `None` when that job does not verify.
    #[must_use]
    pub fn verifies(&self, id: u64) -> Option<Arc<VerifyCount>> {
        self.job_queue.verifies.by_job.get(&id).cloned()
    }

    /// Flips verify for the pending plan and returns the new state; false and no change when no
    /// eligible plan is pending.
    pub fn toggle_verify(&mut self) -> bool {
        let Some(id) = self.pending.as_ref().filter(|p| eligible(p)).map(|p| p.id) else {
            return false;
        };
        let verifies = &mut self.job_queue.verifies;
        if verifies.by_job.remove(&id).is_some() {
            verifies.declined.insert(id);
            return false;
        }
        verifies.declined.remove(&id);
        verifies.by_job.insert(id, Arc::new(VerifyCount::default()));
        true
    }

    /// Drops the choices of jobs that are gone. A re-plan keeps the user's choice; a new plan
    /// gets a fresh counter when eligible and the mode verifies a copy to `class`.
    pub(super) fn offer_verify(&mut self, pending: &PendingOp, class: DriveClass) {
        let jobs = &self.jobs;
        let verifies = &mut self.job_queue.verifies;
        let kept = |id: u64| id == pending.id || jobs.iter().any(|job| job.id == id);
        verifies.by_job.retain(|&id, _| kept(id));
        verifies.declined.retain(|&id| kept(id));
        let chosen =
            verifies.by_job.contains_key(&pending.id) || verifies.declined.contains(&pending.id);
        if !chosen && eligible(pending) && verifies.mode.verifies(class) {
            verifies
                .by_job
                .insert(pending.id, Arc::new(VerifyCount::default()));
        }
    }
}

impl JobWork {
    /// Runs a plain or streamed plan with this job's workers, sources and verify counter.
    pub(super) fn run_plain(&self, ctx: &mut JobContext<'_>) -> JobOutcome {
        let pending = &self.pending;
        let sources = pending.stream.as_ref().map(|stream| &stream.sources);
        // Streamed items are served one at a time, so one worker reads them.
        let workers = if sources.is_some() {
            1
        } else {
            worker_count(
                self.copying,
                pending.plan.kind,
                pending.sources.first(),
                pending.target.as_ref(),
            )
        };
        let options = JobOptions {
            workers,
            sources,
            verify: self.verify.as_deref(),
            speed_cap: None,
        };
        run_job_with(&pending.plan, &[], ctx, options)
    }
}
