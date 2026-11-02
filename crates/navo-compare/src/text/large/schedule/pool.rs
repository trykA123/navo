use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::anchor::SectionPair;
use crate::model::Reason;

use super::super::batch::{self, Batch, Context, Counts, Env, Outcome};
use super::super::budget::{Limits, MAX_EXTRA_WORKERS};

pub(crate) type StopFn = Arc<dyn Fn() -> bool + Send + Sync>;
pub(crate) type Finished = Arc<dyn Fn(u64) + Send + Sync>;

/// One worker's current job and section, read by the watchdog.
#[derive(Default)]
pub(crate) struct Beat {
    job: AtomicU64,
    abandoned: AtomicU64,
    section: Mutex<Option<(SectionPair, Instant)>>,
}

impl Beat {
    pub(crate) fn start_section(&self, pair: &SectionPair) {
        *self.lock() = Some((pair.clone(), Instant::now()));
    }

    pub(crate) fn end_section(&self) {
        *self.lock() = None;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<(SectionPair, Instant)>> {
        self.section.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn abandoned(&self) -> bool {
        let job = self.job.load(Ordering::Acquire);
        job != 0 && self.abandoned.load(Ordering::Acquire) == job
    }

    fn overdue(&self, job: u64, limit: Duration) -> Option<SectionPair> {
        if self.job.load(Ordering::Acquire) != job + 1 {
            return None;
        }
        let section = self.lock();
        let (pair, started) = section.as_ref()?;
        (started.elapsed() >= limit).then(|| pair.clone())
    }
}

pub(super) struct Job {
    pub(super) id: u64,
    pub(super) batch: Batch,
    pub(super) forced: Vec<SectionPair>,
}

pub(super) struct Reply {
    pub(super) worker: usize,
    pub(super) id: u64,
    pub(super) outcome: Result<Outcome, Reason>,
}

pub(super) struct Common {
    pub(super) context: Arc<Context>,
    pub(super) limits: Arc<Limits>,
    pub(super) halt: StopFn,
    pub(super) finished: Option<Finished>,
}

struct Hand {
    jobs: Option<SyncSender<Job>>,
    beat: Arc<Beat>,
    handle: Option<JoinHandle<()>>,
    job: Option<u64>,
}

/// Section workers; each takes one job at a time, so occupancy is exact.
pub(super) struct Pool {
    hands: Vec<Hand>,
    recoveries: Vec<JoinHandle<()>>,
    replies: SyncSender<Reply>,
    common: Arc<Common>,
    extras: Arc<AtomicU32>,
}

impl Pool {
    pub(super) fn new(common: Common, replies: SyncSender<Reply>, extras: Arc<AtomicU32>) -> Self {
        Self {
            hands: Vec::new(),
            recoveries: Vec::new(),
            replies,
            common: Arc::new(common),
            extras,
        }
    }

    pub(super) fn spawn(&mut self) -> Result<(), Reason> {
        let (jobs, receiver) = mpsc::sync_channel(1);
        let beat = Arc::new(Beat::default());
        let worker = Worker {
            index: self.hands.len(),
            common: Arc::clone(&self.common),
            beat: Arc::clone(&beat),
            jobs: receiver,
            replies: self.replies.clone(),
        };
        let handle = thread::Builder::new()
            .name("large-diff-section".to_owned())
            .spawn(move || worker.run())
            .map_err(|_| Reason::WorkerPanicked)?;
        self.hands.push(Hand {
            jobs: Some(jobs),
            beat,
            handle: Some(handle),
            job: None,
        });
        Ok(())
    }

    /// Adds a fresh worker while the diff has extras left; false once they are used.
    pub(super) fn spawn_extra(&mut self) -> Result<bool, Reason> {
        let taken = self
            .extras
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                (used < MAX_EXTRA_WORKERS).then_some(used + 1)
            });
        if taken.is_err() {
            return Ok(false);
        }
        if self.spawn().is_err() {
            let _ = self
                .extras
                .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                    Some(used.saturating_sub(1))
                });
            return Err(Reason::WorkerPanicked);
        }
        Ok(true)
    }

    pub(super) fn expired(&self) -> Option<&Finished> {
        self.common.limits.expired.as_ref()
    }

    pub(super) fn idle(&self) -> Option<usize> {
        self.hands.iter().position(|hand| hand.job.is_none())
    }

    /// Hands a job to an idle worker without blocking; false when it was not taken.
    pub(super) fn dispatch(&mut self, worker: usize, job: Job) -> Result<bool, Reason> {
        let hand = self.hands.get_mut(worker).ok_or(Reason::WorkerPanicked)?;
        let id = job.id;
        let sender = hand.jobs.as_ref().ok_or(Reason::WorkerPanicked)?;
        match sender.try_send(job) {
            Ok(()) => {
                hand.job = Some(id);
                Ok(true)
            }
            Err(TrySendError::Full(_)) => Ok(false),
            Err(TrySendError::Disconnected(_)) => Err(Reason::WorkerPanicked),
        }
    }

    pub(super) fn free(&mut self, worker: usize, id: u64) {
        if let Some(hand) = self.hands.get_mut(worker)
            && hand.job == Some(id)
        {
            hand.job = None;
        }
    }

    pub(super) fn overdue(&self, worker: usize, id: u64, limit: Duration) -> Option<SectionPair> {
        self.hands.get(worker)?.beat.overdue(id, limit)
    }

    pub(super) fn abandon(&self, worker: usize, id: u64) {
        if let Some(hand) = self.hands.get(worker) {
            hand.beat.abandoned.store(id + 1, Ordering::Release);
        }
    }

    pub(super) fn recovering(&self) -> usize {
        self.recoveries
            .iter()
            .filter(|handle| !handle.is_finished())
            .count()
    }

    pub(super) fn recover_job(&mut self, job: Job) -> Result<(), Reason> {
        let common = Arc::clone(&self.common);
        let replies = self.replies.clone();
        let beat = Arc::new(Beat::default());
        let handle = thread::Builder::new()
            .name("large-diff-recovery".to_owned())
            .spawn(move || {
                let stop = || (common.halt)();
                let env = Env {
                    context: &common.context,
                    limits: &common.limits,
                    stop: &stop,
                    beat: &beat,
                    forced: &job.forced,
                };
                let outcome =
                    catch_unwind(AssertUnwindSafe(
                        || match super::super::budget::coarse_batch(&env, &job.batch)? {
                            Some((out, sections)) => Ok(Outcome::Covered {
                                out,
                                counts: Counts {
                                    sections,
                                    dropped: 0,
                                },
                            }),
                            None => Ok(Outcome::Retry),
                        },
                    ))
                    .unwrap_or(Err(Reason::WorkerPanicked));
                let reply = Reply {
                    worker: usize::MAX,
                    id: job.id,
                    outcome,
                };
                let _sent = replies.send(reply);
            })
            .map_err(|_| Reason::WorkerPanicked)?;
        self.recoveries.push(handle);
        Ok(())
    }

    pub(super) fn reap(&mut self) {
        for hand in &mut self.hands {
            if hand.handle.as_ref().is_some_and(JoinHandle::is_finished)
                && let Some(handle) = hand.handle.take()
            {
                let _joined = handle.join();
            }
        }
        let (done, late): (Vec<_>, Vec<_>) = std::mem::take(&mut self.recoveries)
            .into_iter()
            .partition(JoinHandle::is_finished);
        self.recoveries = late;
        for handle in done {
            let _joined = handle.join();
        }
    }

    /// Closes every job queue and returns the workers still finishing a late job.
    pub(super) fn close(mut self) -> Vec<JoinHandle<()>> {
        for hand in &mut self.hands {
            hand.jobs = None;
        }
        self.reap();
        self.hands
            .into_iter()
            .filter_map(|hand| hand.handle)
            .chain(self.recoveries)
            .collect()
    }
}

struct Worker {
    index: usize,
    common: Arc<Common>,
    beat: Arc<Beat>,
    jobs: Receiver<Job>,
    replies: SyncSender<Reply>,
}

impl Worker {
    fn run(self) {
        while let Ok(job) = self.jobs.recv() {
            self.beat.job.store(job.id + 1, Ordering::Release);
            let outcome = self.attempt(&job);
            self.beat.end_section();
            self.beat.job.store(0, Ordering::Release);
            let id = job.id;
            let reply = Reply {
                worker: self.index,
                id,
                outcome,
            };
            drop(job);
            let sent = self.replies.send(reply).is_ok();
            if let Some(finished) = &self.common.finished {
                finished(id);
            }
            if !sent {
                return;
            }
        }
    }

    fn attempt(&self, job: &Job) -> Result<Outcome, Reason> {
        let (common, beat) = (&self.common, &self.beat);
        let stop = || (common.halt)() || beat.abandoned();
        let env = Env {
            context: &common.context,
            limits: &common.limits,
            stop: &stop,
            beat: &self.beat,
            forced: &job.forced,
        };
        catch_unwind(AssertUnwindSafe(|| batch::run(&env, &job.batch)))
            .unwrap_or(Err(Reason::WorkerPanicked))
    }
}
