use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Mutex, PoisonError};
use std::thread;

use crate::anchor::SectionPair;
use crate::model::Reason;

use super::batch::{self, Batch, Context, Counts, Outcome, Stop};
use super::{AHEAD, Covered, DiffEvent, Driver, Stage, WORKERS};

impl Driver {
    pub(super) fn sections_phase(
        &self,
        context: &Context,
        schedule: &mut Schedule,
    ) -> Result<(), Reason> {
        let halt = AtomicBool::new(false);
        let stop = || self.cancel.is_cancelled() || halt.load(Ordering::Acquire);
        let (jobs, job_receiver) = mpsc::sync_channel::<Job>(WORKERS);
        let (replies, reply_receiver) = mpsc::sync_channel::<Reply>(WORKERS);
        let job_receiver = Mutex::new(job_receiver);
        thread::scope(|scope| {
            let started = (0..WORKERS.min(schedule.slots.len())).try_for_each(|_| {
                let replies = replies.clone();
                let (jobs, stop) = (&job_receiver, &stop);
                thread::Builder::new()
                    .name("large-diff-section".to_owned())
                    .spawn_scoped(scope, move || worker(context, jobs, &replies, stop))
                    .map(drop)
            });
            drop(replies);
            let result = match started {
                Ok(()) => self.schedule(schedule, &jobs, &reply_receiver),
                Err(_) => Err(Reason::WorkerPanicked),
            };
            halt.store(true, Ordering::Release);
            drop(jobs);
            drop(reply_receiver);
            result
        })
    }

    fn schedule(
        &self,
        state: &mut Schedule,
        jobs: &SyncSender<Job>,
        replies: &Receiver<Reply>,
    ) -> Result<(), Reason> {
        loop {
            self.flush(state)?;
            self.fill(state, jobs)?;
            if state.in_flight == 0 {
                return state.finished();
            }
            let reply = replies.recv().map_err(|_| Reason::WorkerPanicked)?;
            state.in_flight -= 1;
            self.handle(state, reply)?;
        }
    }

    fn fill(&self, state: &mut Schedule, jobs: &SyncSender<Job>) -> Result<(), Reason> {
        while state.in_flight < WORKERS {
            if self.cancel.is_cancelled() {
                return Err(Reason::Stopped);
            }
            let Some(index) = state.pick(self.focus.load(Ordering::Relaxed)) else {
                break;
            };
            let job = state.dispatch(index);
            jobs.send(job).map_err(|_| Reason::WorkerPanicked)?;
        }
        Ok(())
    }

    fn handle(&self, state: &mut Schedule, reply: Reply) -> Result<(), Reason> {
        let focused = state.focus_job == Some(reply.id);
        if focused {
            state.focus_job = None;
        }
        let Some(index) = state.running(reply.id) else {
            return Ok(());
        };
        match reply.outcome? {
            Outcome::Covered { covered, counts } => {
                state.slots[index].state = State::Ready(covered, counts);
                if focused {
                    self.emit(state, index)?;
                }
            }
            Outcome::Collision { start, end } => {
                if end {
                    state.merge(index)?;
                }
                let merged = if start {
                    index.checked_sub(1).ok_or(Reason::ChangedOnDisk)?
                } else {
                    index
                };
                if start {
                    state.merge(merged)?;
                }
                if focused {
                    state.refocus = Some(merged);
                }
            }
        }
        Ok(())
    }

    fn flush(&self, state: &mut Schedule) -> Result<(), Reason> {
        if state.focus_job.is_some() || state.refocus.is_some() {
            return Ok(());
        }
        while let Some(slot) = state.slots.get(state.next_emit) {
            match slot.state {
                State::Ready(..) => self.emit(state, state.next_emit)?,
                State::Emitted => {}
                State::Pending | State::Running(_) => return Ok(()),
            }
            state.next_emit += 1;
        }
        Ok(())
    }

    fn emit(&self, state: &mut Schedule, index: usize) -> Result<(), Reason> {
        let slot = &mut state.slots[index];
        let State::Ready(covered, counts) = std::mem::replace(&mut slot.state, State::Emitted)
        else {
            return Ok(());
        };
        state.counts.sections += counts.sections;
        state.counts.dropped += counts.dropped;
        state.emitted = state.emitted.saturating_add(slot.batch.lines());
        self.send(DiffEvent::Hunks(covered))?;
        self.hint(Stage::Sections, state.emitted, state.total);
        Ok(())
    }
}

pub(super) fn group(sections: Vec<SectionPair>, limit: u64, terminal: bool) -> Vec<Batch> {
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut size = 0_u64;
    for section in sections {
        let lines =
            (section.left.end - section.left.start).max(section.right.end - section.right.start);
        if !current.is_empty() && size.saturating_add(lines) > limit {
            batches.push(Batch {
                sections: std::mem::take(&mut current),
                start_anchor: !batches.is_empty(),
                end_anchor: true,
                terminal: false,
            });
            size = 0;
        }
        size = size.saturating_add(lines);
        current.push(section);
    }
    if !current.is_empty() {
        batches.push(Batch {
            sections: current,
            start_anchor: !batches.is_empty(),
            end_anchor: false,
            terminal,
        });
    }
    batches
}

struct Job {
    id: u64,
    batch: Batch,
}

struct Reply {
    id: u64,
    outcome: Result<Outcome, Reason>,
}

fn worker(
    context: &Context,
    jobs: &Mutex<Receiver<Job>>,
    replies: &SyncSender<Reply>,
    stop: Stop<'_>,
) {
    while !stop() {
        let job = jobs.lock().unwrap_or_else(PoisonError::into_inner).recv();
        let Ok(job) = job else {
            return;
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| batch::run(context, &job.batch, stop)))
            .unwrap_or(Err(Reason::WorkerPanicked));
        if replies
            .send(Reply {
                id: job.id,
                outcome,
            })
            .is_err()
        {
            return;
        }
    }
}

enum State {
    Pending,
    Running(u64),
    Ready(Covered, Counts),
    Emitted,
}

struct Slot {
    batch: Batch,
    state: State,
}

pub(super) struct Schedule {
    slots: Vec<Slot>,
    next_emit: usize,
    focus_job: Option<u64>,
    refocus: Option<usize>,
    next_id: u64,
    in_flight: usize,
    pub(super) counts: Counts,
    emitted: u64,
    total: u64,
}

impl Schedule {
    pub(super) fn new(batches: Vec<Batch>) -> Self {
        let total = batches.iter().map(Batch::lines).sum();
        Self {
            slots: batches
                .into_iter()
                .map(|batch| Slot {
                    batch,
                    state: State::Pending,
                })
                .collect(),
            next_emit: 0,
            focus_job: None,
            refocus: None,
            next_id: 0,
            in_flight: 0,
            counts: Counts::default(),
            emitted: 0,
            total,
        }
    }

    fn pick(&mut self, focus: u64) -> Option<usize> {
        if let Some(index) = self.refocus.take() {
            self.focus_job = Some(self.next_id);
            return Some(index);
        }
        let last = self.slots.len().checked_sub(1)?;
        let focused = self
            .slots
            .partition_point(|slot| slot.batch.left().end <= focus)
            .min(last);
        if self.focus_job.is_none() && matches!(self.slots[focused].state, State::Pending) {
            self.focus_job = Some(self.next_id);
            return Some(focused);
        }
        let end = self.slots.len().min(self.next_emit + AHEAD);
        (self.next_emit..end).find(|&index| matches!(self.slots[index].state, State::Pending))
    }

    fn finished(&self) -> Result<(), Reason> {
        if self.next_emit == self.slots.len() {
            Ok(())
        } else {
            Err(Reason::WorkerPanicked)
        }
    }

    fn dispatch(&mut self, index: usize) -> Job {
        let id = self.next_id;
        self.next_id += 1;
        self.in_flight += 1;
        let slot = &mut self.slots[index];
        slot.state = State::Running(id);
        Job {
            id,
            batch: slot.batch.clone(),
        }
    }

    fn running(&self, id: u64) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| matches!(slot.state, State::Running(running) if running == id))
    }

    fn merge(&mut self, index: usize) -> Result<(), Reason> {
        let emitted = |slot: &Slot| matches!(slot.state, State::Emitted);
        if index + 1 >= self.slots.len() || self.slots[index..=index + 1].iter().any(emitted) {
            return Err(Reason::ChangedOnDisk);
        }
        let next = self.slots.remove(index + 1);
        let slot = &mut self.slots[index];
        slot.batch.sections.extend(next.batch.sections);
        slot.batch.end_anchor = next.batch.end_anchor;
        slot.batch.terminal = next.batch.terminal;
        slot.state = State::Pending;
        Ok(())
    }
}
