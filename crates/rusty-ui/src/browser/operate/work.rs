use rusty_core::{ConflictPolicy, FsError, Location, OperationPlan, PlanRequest, plan};
use rusty_fs::{
    AutoTuning, CancelToken, CopyMode, DriveClass, JobContext, JobOutcome, Progress, Provider,
    ProviderOps, WorkerCounts, existing_at, job_workers, run_job_parallel, scan,
};

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use super::{Copying, JobUpdate, LogFactory, OpRequest};
use crate::browser::{Message, Waker};

const PROGRESS_EVERY: Duration = Duration::from_millis(50);

pub(super) fn lands_inside(target: &Location, folder: &Location, sources: &[Location]) -> bool {
    if target == folder {
        return true;
    }
    let mut current = Some(target.clone());
    while let Some(location) = current {
        if sources.contains(&location) {
            return true;
        }
        current = location.parent();
    }
    false
}

pub(super) fn start_progress(plan: &OperationPlan) -> Progress {
    Progress {
        bytes_done: 0,
        bytes_total: plan.totals.bytes,
        files_done: 0,
        files_total: plan.totals.files,
        current: None,
        paused: false,
    }
}

pub(super) fn plan_for(
    provider: &dyn Provider,
    request: &OpRequest,
    cancel: &CancelToken,
) -> Result<OperationPlan, FsError> {
    let items = scan(provider, &request.sources, cancel)?;
    let existing = match &request.target {
        Some(target) => existing_at(provider, &items, target, cancel)?,
        None => Vec::new(),
    };
    Ok(plan(&PlanRequest {
        kind: request.kind,
        items,
        target: request.target.clone(),
        existing,
        policy: ConflictPolicy::Ask,
        same_volume: request.same_volume,
        protected: request.protected.clone(),
    }))
}

pub(super) fn worker_count(
    mode: CopyMode,
    source: Option<&Location>,
    target: Option<&Location>,
    class_of: impl Fn(&Location) -> DriveClass,
) -> usize {
    let class = |place: Option<&Location>| place.map_or(DriveClass::Unknown, &class_of);
    let counts = WorkerCounts::from(&AutoTuning::default());
    job_workers(mode, class(source), class(target), counts)
}

pub(super) struct JobWork {
    pub id: u64,
    pub copying: Copying,
    pub source: Option<Location>,
    pub target: Option<Location>,
    pub plan: OperationPlan,
    pub ops: Arc<dyn ProviderOps>,
    pub factory: LogFactory,
    pub cancel: CancelToken,
    pub sender: Sender<Message>,
    pub wake: Waker,
}

struct Reporter<'a> {
    id: u64,
    sender: &'a Sender<Message>,
    wake: &'a Waker,
    sent: Instant,
    last: Option<Progress>,
}

impl Reporter<'_> {
    fn report(&mut self, progress: &Progress) {
        let was_paused = self.last.as_ref().is_some_and(|last| last.paused);
        self.last = Some(progress.clone());
        if progress.paused == was_paused && self.sent.elapsed() < PROGRESS_EVERY {
            return;
        }
        self.sent = Instant::now();
        let progress = progress.clone();
        let id = self.id;
        let _ = self
            .sender
            .send(Message::Job(JobUpdate::Progress { id, progress }));
        (self.wake)();
    }
}

impl JobWork {
    pub(super) fn run(self) {
        let mut reporter = Reporter {
            id: self.id,
            sender: &self.sender,
            wake: &self.wake,
            sent: Instant::now(),
            last: None,
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            self.execute(&mut |progress| reporter.report(progress))
        }))
        .unwrap_or_else(|_| Err("The job stopped unexpectedly".to_owned()));
        let _ = self.sender.send(Message::Job(JobUpdate::Done {
            id: self.id,
            progress: reporter.last,
            outcome,
        }));
        (self.wake)();
    }

    fn execute(&self, progress: &mut dyn FnMut(&Progress)) -> Result<JobOutcome, String> {
        let Copying { mode, class_of } = self.copying;
        let workers = worker_count(mode, self.source.as_ref(), self.target.as_ref(), class_of);
        let mut log = (self.factory)(&self.plan)?;
        let outcome = run_job_parallel(
            &self.plan,
            &[],
            &mut JobContext {
                ops: self.ops.as_ref(),
                log: log.as_mut(),
                cancel: &self.cancel,
                progress,
            },
            workers,
        );
        log.finish(&outcome, self.ops.as_ref());
        Ok(outcome)
    }
}
