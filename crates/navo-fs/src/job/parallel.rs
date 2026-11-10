use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{Builder, scope};

use navo_core::{FsError, FsErrorKind, Location, OpKind, OperationPlan, Step};

use super::aside::Aside;
use super::copy::{Copier, CopyStep, failure};
use super::marks::{Marks, is_removal};
use super::pool::{Event, Task, work};
use super::remove::Removals;
use super::run::{Job, apply, copier_of, counted, current_of, holds_kept, namespace_of, note_kept};
use super::{JobContext, JobOutcome, Progress, Sources, StepOutcome};

mod events;

pub(super) fn run(
    plan: &OperationPlan,
    done: &[u32],
    ctx: &mut JobContext<'_>,
    workers: usize,
) -> JobOutcome {
    run_with((plan, None), done, ctx, workers)
}

pub(super) fn run_with(
    (plan, sources): (&OperationPlan, Option<&Sources>),
    done: &[u32],
    ctx: &mut JobContext<'_>,
    workers: usize,
) -> JobOutcome {
    let pauses = match super::git_pause::pause_job(plan, ctx) {
        Ok(pauses) => pauses,
        Err(error) => {
            let seq = (0_u32..)
                .zip(&plan.steps)
                .find(|(seq, _)| !done.contains(seq))
                .map_or(0, |(seq, _)| seq);
            return JobOutcome::Failed { seq, error };
        }
    };
    let aside = Aside::new();
    let removals = Removals::of(plan);
    let namespace = namespace_of(ctx);
    let job = Job {
        plan,
        aside: &aside,
        namespace: namespace.as_deref(),
        removals: &removals,
        sources,
    };
    if let Err((seq, error)) = super::run::precheck_writes(job, done, ctx) {
        return if error.kind == FsErrorKind::Cancelled {
            JobOutcome::Cancelled
        } else {
            JobOutcome::Failed { seq, error }
        };
    }
    coordinate((job, copier_of(job, ctx), pauses), ctx, done, workers)
}

fn coordinate<'p>(
    (job, copier, pauses): (Job<'p>, Copier<'p>, Vec<crate::GitPauseGuard>),
    ctx: &mut JobContext<'_>,
    done: &[u32],
    workers: usize,
) -> JobOutcome {
    let plan = job.plan;
    let (tasks, queue) = channel();
    let queue = Mutex::new(queue);
    let (events, finished) = channel();
    scope(|scope| {
        let mut started = 0;
        for _ in 0..workers {
            let (copier, queue, events) = (&copier, &queue, events.clone());
            let spawned = Builder::new()
                .name("navo-copy".to_owned())
                .spawn_scoped(scope, move || work(copier, queue, &events));
            started += usize::from(spawned.is_ok());
        }
        drop(events);
        if started == 0 {
            drop(pauses);
            return super::run::run_with((plan, job.sources), done, ctx);
        }
        let pool = Pool {
            tasks,
            events: finished,
            workers: started,
        };
        let mut runner = Runner::new(job, ctx, pool, pauses);
        runner.walk(done);
        runner.outcome()
    })
}

struct Pool<'p> {
    tasks: Sender<Task<'p>>,
    events: Receiver<Event>,
    workers: usize,
}

struct Runner<'p, 'c, 'a> {
    job: Job<'p>,
    ctx: &'c mut JobContext<'a>,
    pool: Pool<'p>,
    in_flight: HashMap<u32, (&'p Step, u64)>,
    settled: u64,
    progress: Progress,
    kept: Vec<Location>,
    marks: Marks,
    pauses: Option<Vec<crate::GitPauseGuard>>,
    failed: Option<(u32, FsError)>,
    partial: Option<(u32, FsError)>,
    cancelled: bool,
}

fn names_clash(
    (one, seq): (&Location, u32),
    (other, other_seq): (&Location, u32),
    namespace: Option<&str>,
) -> bool {
    let temporary = |target, seq| navo_core::retention::copy_temporary(target, seq, namespace).ok();
    one.same_place(other)
        || temporary(one, seq).is_some_and(|temp| temp.same_place(other))
        || temporary(other, other_seq).is_some_and(|temp| temp.same_place(one))
}

fn room_for(step: &Step, workers: usize) -> usize {
    match step {
        Step::CopyFile { .. } | Step::RemoveFile { .. } | Step::RemoveLink { .. } => {
            workers.saturating_sub(1)
        }
        Step::MakeFolder { .. } | Step::RemoveFolder { .. } => usize::MAX,
        _ => 0,
    }
}

fn runs_on_the_pool_as_a_removal(kind: OpKind, step: &Step) -> bool {
    kind == OpKind::Delete && matches!(step, Step::RemoveFile { .. } | Step::RemoveLink { .. })
}

fn inside(child: &Location, folder: &Location) -> bool {
    std::iter::successors(child.parent(), Location::parent).any(|up| up.same_place(folder))
}

impl<'p, 'c, 'a> Runner<'p, 'c, 'a> {
    fn new(
        job: Job<'p>,
        ctx: &'c mut JobContext<'a>,
        pool: Pool<'p>,
        pauses: Vec<crate::GitPauseGuard>,
    ) -> Self {
        let plan = job.plan;
        let progress = Progress::start(plan);
        Self {
            job,
            ctx,
            pool,
            in_flight: HashMap::new(),
            settled: 0,
            progress,
            kept: Vec::new(),
            marks: Marks::new(plan.kind),
            pauses: Some(pauses),
            failed: None,
            partial: None,
            cancelled: false,
        }
    }

    fn walk(&mut self, done: &[u32]) {
        let plan = self.job.plan;
        for (seq, step) in (0_u32..).zip(&plan.steps) {
            if done.contains(&seq) {
                self.count(step, counted(self.job.plan.kind, step).0);
                continue;
            }
            self.wait_for_room((seq, step));
            if self.stopped() {
                break;
            }
            self.start(seq, step);
        }
        while !self.in_flight.is_empty() {
            self.receive();
            self.note_pause();
            if self.progress.paused {
                self.flush_marks();
            }
            if self.progress.paused && self.pauses.is_some() && self.no_removal_in_flight() {
                self.pauses = None;
            }
        }
        self.flush_marks();
    }

    fn flush_marks(&mut self) {
        if let Err((seq, error)) = self.marks.flush(self.ctx.log) {
            self.fail(seq, error);
        }
    }

    fn outcome(self) -> JobOutcome {
        if self.cancelled {
            return JobOutcome::Cancelled;
        }
        match self.failed.or(self.partial) {
            Some((seq, error)) => JobOutcome::Failed { seq, error },
            None => JobOutcome::Done,
        }
    }

    fn stopped(&mut self) -> bool {
        self.cancelled |= self.ctx.cancel.is_cancelled();
        self.cancelled || self.failed.is_some()
    }

    fn wait_for_room(&mut self, (seq, step): (u32, &Step)) {
        let room = room_for(step, self.pool.workers);
        loop {
            self.note_pause();
            if self.progress.paused {
                self.flush_marks();
            }
            if self.progress.paused && self.pauses.is_some() && self.no_removal_in_flight() {
                self.pauses = None;
            }
            let is_busy = self.in_flight.len() > room
                || self.progress.paused
                || self.shares_a_name((seq, step))
                || self.waits_for_others(step);
            if self.stopped() || !is_busy {
                self.retake_if_needed(seq);
                return;
            }
            if self.pauses.is_none() {
                self.resume_after_pause(seq);
            } else if self.in_flight.is_empty() {
                self.flush_marks();
                self.resume_after_pause(seq);
            } else {
                self.receive();
            }
        }
    }

    /// Drops the pause while the job sleeps, then takes it again before the next step.
    fn resume_after_pause(&mut self, seq: u32) {
        if self.progress.paused {
            self.pauses = None;
        }
        self.ctx.cancel.wait_while_paused();
        if !self.stopped() {
            self.retake(seq);
        }
    }

    /// Takes the job-wide pause again unless the job has stopped.
    fn retake_if_needed(&mut self, seq: u32) {
        if !self.stopped() {
            self.retake(seq);
        }
    }

    /// Takes the job-wide pause again before a mutating step.
    fn retake(&mut self, seq: u32) {
        if self.pauses.is_some() {
            return;
        }
        match super::git_pause::pause_job(self.job.plan, self.ctx) {
            Ok(pauses) => self.pauses = Some(pauses),
            Err(error) => self.fail(seq, error),
        }
    }

    /// Whether every in-flight task is a copy, so no removal or rename is running.
    fn no_removal_in_flight(&self) -> bool {
        self.in_flight.values().all(|(step, _)| !is_removal(step))
    }

    fn shares_a_name(&self, (seq, step): (u32, &Step)) -> bool {
        let Step::CopyFile { target, .. } = step else {
            return false;
        };
        self.in_flight
            .iter()
            .any(|(other_seq, (other, _))| match other {
                Step::CopyFile { target: busy, .. } => {
                    names_clash((target, seq), (busy, *other_seq), self.job.namespace)
                }
                _ => false,
            })
    }

    fn waits_for_others(&self, step: &Step) -> bool {
        let mut busy = self.in_flight.values().map(|(other, _)| *other);
        match step {
            Step::CopyFile { .. } => busy.any(is_removal),
            Step::RemoveFile { .. } | Step::RemoveLink { .. } => {
                busy.any(|other| !is_removal(other))
            }
            Step::RemoveFolder { location } => busy.any(|other| {
                !is_removal(other) || inside(&super::run::current_of(other), location)
            }),
            _ => false,
        }
    }

    fn note_pause(&mut self) {
        let cancel = self.ctx.cancel;
        loop {
            let paused = cancel.is_paused() && !cancel.is_cancelled();
            if paused == self.progress.paused {
                return;
            }
            self.progress.paused = paused;
            if paused {
                self.flush_marks();
            }
            self.report();
        }
    }

    fn start(&mut self, seq: u32, step: &'p Step) {
        self.progress.current = Some(current_of(step));
        if runs_on_the_pool_as_a_removal(self.job.plan.kind, step) && !holds_kept(step, &self.kept)
        {
            self.dispatch(seq, step, Task::Remove { seq, step });
            return;
        }
        let Step::CopyFile {
            source,
            target,
            size,
        } = step
        else {
            let result = if holds_kept(step, &self.kept) {
                Ok(StepOutcome::Skipped)
            } else {
                self.apply_step(seq, step)
            };
            let bytes = counted(self.job.plan.kind, step).0;
            self.settle(seq, step, result.map(|outcome| (outcome, bytes)));
            return;
        };
        let copy = CopyStep {
            seq,
            source,
            target,
            size: *size,
        };
        if let Err(error) = super::temporary::prepare(self.ctx, &copy, self.job.namespace) {
            self.fail(seq, error);
            return;
        }
        self.dispatch(seq, step, Task::Copy { seq, step: copy });
    }

    /// Applies a step that stays on the runner thread; parallel copies own the guard.
    fn apply_step(&mut self, seq: u32, step: &'p Step) -> Result<StepOutcome, FsError> {
        let pauses = std::cell::RefCell::new(None);
        super::rename::prepare(seq, (self.job.plan, step), self.ctx)
            .and_then(|()| apply(self.job, (seq, step), self.ctx, &mut self.progress, &pauses))
    }

    fn dispatch(&mut self, seq: u32, step: &'p Step, task: Task<'p>) {
        self.in_flight.insert(seq, (step, 0));
        if self.pool.tasks.send(task).is_err() {
            self.in_flight.remove(&seq);
            self.fail(seq, failure(FsErrorKind::Unknown, &current_of(step)));
        }
    }

    fn note_failure(&mut self, seq: u32, error: FsError) {
        if self.partial.as_ref().is_none_or(|(first, _)| seq < *first) {
            self.partial = Some((seq, error));
        }
    }

    fn fail(&mut self, seq: u32, error: FsError) {
        if self.failed.as_ref().is_none_or(|(first, _)| seq < *first) {
            self.failed = Some((seq, error));
        }
    }

    fn count(&mut self, step: &Step, bytes: u64) {
        let files = counted(self.job.plan.kind, step).1;
        self.settled += bytes;
        self.progress.files_done += files;
    }

    fn report(&mut self) {
        let in_flight: u64 = self.in_flight.values().map(|(_, bytes)| bytes).sum();
        self.progress.bytes_done = self.progress.bytes_done.max(self.settled + in_flight);
        (self.ctx.progress)(&self.progress);
    }
}

#[cfg(test)]
mod tests;
