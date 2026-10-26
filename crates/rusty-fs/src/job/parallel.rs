use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{Builder, scope};

use rusty_core::{FsError, FsErrorKind, Location, OperationPlan, Step};

use super::aside::Aside;
use super::copy::{Copied, Copier, CopyStep, failure, place, temp_of};
use super::pool::{Event, Task, work};
use super::run::{Job, apply, counted, current_of, holds_kept, note_kept};
use super::{JobContext, JobOutcome, Progress, StepOutcome};

pub(super) fn run(
    plan: &OperationPlan,
    done: &[u32],
    ctx: &mut JobContext<'_>,
    workers: usize,
) -> JobOutcome {
    let aside = Aside::new();
    let job = Job {
        plan,
        aside: &aside,
    };
    let copier = Copier {
        plan,
        ops: ctx.ops,
        cancel: ctx.cancel,
    };
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
            return super::run::run(plan, done, ctx);
        }
        let pool = Pool {
            tasks,
            events: finished,
            workers: started,
        };
        let mut runner = Runner::new(job, ctx, pool);
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
    failed: Option<(u32, FsError)>,
    cancelled: bool,
}

fn names_clash(one: &Location, other: &Location) -> bool {
    let is_temp_of = |temp: &Location, of: &Location| {
        temp_of(of).is_ok_and(|candidate| candidate.same_place(temp))
    };
    one.same_place(other) || is_temp_of(one, other) || is_temp_of(other, one)
}

fn room_for(step: &Step, workers: usize) -> usize {
    match step {
        Step::CopyFile { .. } => workers.saturating_sub(1),
        Step::MakeFolder { .. } => usize::MAX,
        _ => 0,
    }
}

impl<'p, 'c, 'a> Runner<'p, 'c, 'a> {
    fn new(job: Job<'p>, ctx: &'c mut JobContext<'a>, pool: Pool<'p>) -> Self {
        let plan = job.plan;
        let progress = Progress {
            bytes_done: 0,
            bytes_total: plan.totals.bytes,
            files_done: 0,
            files_total: plan.totals.files,
            current: None,
            paused: false,
        };
        Self {
            job,
            ctx,
            pool,
            in_flight: HashMap::new(),
            settled: 0,
            progress,
            kept: Vec::new(),
            failed: None,
            cancelled: false,
        }
    }

    fn walk(&mut self, done: &[u32]) {
        let plan = self.job.plan;
        for (seq, step) in (0_u32..).zip(&plan.steps) {
            if done.contains(&seq) {
                self.count(step);
                continue;
            }
            self.wait_for_room(step);
            if self.stopped() {
                break;
            }
            self.start(seq, step);
        }
        while !self.in_flight.is_empty() {
            self.receive();
            self.note_pause();
        }
    }

    fn outcome(self) -> JobOutcome {
        match self.failed {
            Some((seq, error)) => JobOutcome::Failed { seq, error },
            None if self.cancelled => JobOutcome::Cancelled,
            None => JobOutcome::Done,
        }
    }

    fn stopped(&mut self) -> bool {
        self.cancelled |= self.ctx.cancel.is_cancelled();
        self.cancelled || self.failed.is_some()
    }

    fn wait_for_room(&mut self, step: &Step) {
        let room = room_for(step, self.pool.workers);
        loop {
            self.note_pause();
            let is_busy =
                self.in_flight.len() > room || self.progress.paused || self.shares_a_name(step);
            if self.stopped() || !is_busy {
                return;
            }
            if self.in_flight.is_empty() {
                self.ctx.cancel.wait_while_paused();
            } else {
                self.receive();
            }
        }
    }

    fn shares_a_name(&self, step: &Step) -> bool {
        let Step::CopyFile { target, .. } = step else {
            return false;
        };
        self.in_flight.values().any(|(other, _)| match other {
            Step::CopyFile { target: busy, .. } => names_clash(target, busy),
            _ => false,
        })
    }

    fn note_pause(&mut self) {
        let cancel = self.ctx.cancel;
        let paused = cancel.is_paused() && !cancel.is_cancelled();
        if paused != self.progress.paused {
            self.progress.paused = paused;
            self.report();
        }
    }

    fn start(&mut self, seq: u32, step: &'p Step) {
        self.progress.current = Some(current_of(step));
        let Step::CopyFile {
            source,
            target,
            size,
        } = step
        else {
            let result = if holds_kept(step, &self.kept) {
                Ok(StepOutcome::Skipped)
            } else {
                apply(self.job, step, self.ctx, &mut self.progress)
            };
            self.settle(seq, step, result);
            return;
        };
        let copy = CopyStep {
            source,
            target,
            size: *size,
        };
        self.in_flight.insert(seq, (step, 0));
        if self.pool.tasks.send(Task { seq, step: copy }).is_err() {
            self.in_flight.remove(&seq);
            self.fail(seq, failure(FsErrorKind::Unknown, target));
        }
    }

    fn receive(&mut self) {
        match self.pool.events.recv() {
            Ok(Event::Bytes { seq, bytes }) => {
                if let Some(entry) = self.in_flight.get_mut(&seq) {
                    entry.1 = bytes;
                }
                self.report();
            }
            Ok(Event::Finished { seq, result }) => {
                if let Some((step, _)) = self.in_flight.remove(&seq) {
                    let result = result.and_then(|copied| self.place(step, copied));
                    self.settle(seq, step, result);
                }
            }
            Err(_) => {
                let lost: Vec<u32> = self.in_flight.drain().map(|(seq, _)| seq).collect();
                for seq in lost {
                    self.fail(seq, FsError::new(FsErrorKind::Unknown));
                }
            }
        }
    }

    fn place(&mut self, step: &Step, copied: Copied) -> Result<StepOutcome, FsError> {
        let Step::CopyFile { target, .. } = step else {
            return Err(FsError::new(FsErrorKind::Unknown));
        };
        place(self.job.aside, self.ctx, copied, target)
    }

    fn settle(&mut self, seq: u32, step: &Step, result: Result<StepOutcome, FsError>) {
        match result {
            Ok(outcome) => {
                note_kept(step, outcome, &mut self.kept);
                if let Err(error) = self.ctx.log.step(seq, outcome) {
                    self.fail(seq, error);
                    return;
                }
                self.count(step);
                self.report();
            }
            Err(error)
                if error.kind == FsErrorKind::Cancelled || self.ctx.cancel.is_cancelled() =>
            {
                self.cancelled = true;
            }
            Err(error) => {
                let _ = self.ctx.log.step(seq, StepOutcome::Failed);
                self.fail(seq, error);
            }
        }
    }

    fn fail(&mut self, seq: u32, error: FsError) {
        if self.failed.as_ref().is_none_or(|(first, _)| seq < *first) {
            self.failed = Some((seq, error));
        }
    }

    fn count(&mut self, step: &Step) {
        let (bytes, files) = counted(self.job.plan.kind, step);
        self.settled += bytes;
        self.progress.files_done += files;
    }

    fn report(&mut self) {
        let in_flight: u64 = self.in_flight.values().map(|(_, bytes)| bytes).sum();
        self.progress.bytes_done = self.progress.bytes_done.max(self.settled + in_flight);
        (self.ctx.progress)(&self.progress);
    }
}
