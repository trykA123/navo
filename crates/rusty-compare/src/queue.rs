use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::model::{CancelToken, ReadHint};

mod outcome;

pub use outcome::Settlement;

pub const LARGE_FILE: u64 = 64 * 1024 * 1024;
pub const MAX_WORKERS: usize = 12;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

#[must_use]
pub fn worker_limit() -> usize {
    std::thread::available_parallelism()
        .map_or(1, |count| count.get())
        .min(MAX_WORKERS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub id: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: u64,
    pub read_hint: ReadHint,
    pub cancel: CancelToken,
}

#[derive(Debug, Clone)]
pub enum CheckEvent {
    Job(Job),
    Stopped(u64),
}

#[derive(Debug, Clone, Copy)]
pub struct JobSpec {
    pub id: u64,
    pub size: u64,
    pub volumes: (u64, u64),
}

struct JobState {
    size: u64,
    volumes: (u64, u64),
    active: bool,
    cancel: Option<CancelToken>,
}

#[derive(Default)]
struct State {
    jobs: HashMap<u64, JobState>,
    visible: HashSet<u64>,
    visible_order: BTreeSet<(u64, u64)>,
    other_order: BTreeSet<(u64, u64)>,
    stopped: Vec<u64>,
    active: usize,
    active_volumes: HashMap<u64, usize>,
    last_progress: HashMap<u64, Instant>,
    stopped_all: bool,
    paused: bool,
    closed: bool,
}

pub struct Queue {
    workers: usize,
    state: Mutex<State>,
    wake: Condvar,
}

impl Queue {
    #[must_use]
    pub fn new(workers: usize) -> Self {
        Self {
            workers: workers.max(1),
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
        }
    }

    #[must_use]
    pub const fn workers(&self) -> usize {
        self.workers
    }

    #[must_use]
    pub fn remaining(&self) -> usize {
        self.state().jobs.len()
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn push(&self, id: u64, size: u64, volumes: (u64, u64)) {
        self.push_batch(&[JobSpec { id, size, volumes }]);
    }

    pub fn push_batch(&self, jobs: &[JobSpec]) {
        let mut state = self.state();
        if state.closed {
            return;
        }
        for job in jobs {
            if state.jobs.contains_key(&job.id) {
                continue;
            }
            if state.stopped_all {
                state.stopped.push(job.id);
                continue;
            }
            state.jobs.insert(
                job.id,
                JobState {
                    size: job.size,
                    volumes: job.volumes,
                    active: false,
                    cancel: None,
                },
            );
            if state.visible.contains(&job.id) {
                state.visible_order.insert((job.size, job.id));
            } else {
                state.other_order.insert((job.size, job.id));
            }
        }
        self.wake.notify_all();
    }

    pub fn pause(&self) {
        self.state().paused = true;
    }

    pub fn resume(&self) {
        self.state().paused = false;
        self.wake.notify_all();
    }

    pub fn focus(&self, ids: &[u64]) {
        let mut state = self.state();
        let next: HashSet<u64> = ids.iter().copied().collect();
        let hide: Vec<(u64, u64)> = state
            .visible
            .difference(&next)
            .filter_map(|id| {
                let job = state.jobs.get(id)?;
                (!job.active).then_some((job.size, *id))
            })
            .collect();
        for (size, id) in hide {
            state.visible_order.remove(&(size, id));
            state.other_order.insert((size, id));
        }
        let show: Vec<(u64, u64)> = next
            .difference(&state.visible)
            .filter_map(|id| {
                let job = state.jobs.get(id)?;
                (!job.active).then_some((job.size, *id))
            })
            .collect();
        for (size, id) in show {
            state.other_order.remove(&(size, id));
            state.visible_order.insert((size, id));
        }
        state.visible = next;
        self.wake.notify_all();
    }

    pub fn next(&self) -> Option<CheckEvent> {
        let mut state = self.state();
        loop {
            if state.closed {
                return None;
            }
            if let Some(event) = take_event(&mut state, self.workers) {
                return Some(event);
            }
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    #[must_use]
    pub fn try_next(&self) -> Option<CheckEvent> {
        let mut state = self.state();
        if state.closed {
            return None;
        }
        take_event(&mut state, self.workers)
    }

    pub fn complete(&self, id: u64) -> Settlement {
        let mut state = self.state();
        let Some(job) = state.jobs.remove(&id) else {
            return Settlement::Stopped;
        };
        let settlement = if job.cancel.as_ref().is_some_and(CancelToken::is_cancelled) {
            Settlement::Stopped
        } else {
            Settlement::Checked
        };
        if job.active && job.size > LARGE_FILE {
            decrement_volumes(&mut state, job.volumes);
        }
        state.active = state.active.saturating_sub(usize::from(job.active));
        state.last_progress.remove(&id);
        self.wake.notify_all();
        settlement
    }

    pub fn stop(&self, id: u64) -> bool {
        let mut state = self.state();
        let Some(job) = state.jobs.get_mut(&id) else {
            return false;
        };
        if let Some(cancel) = &job.cancel {
            cancel.cancel();
        }
        if job.active {
            return true;
        }
        let size = job.size;
        state.visible_order.remove(&(size, id));
        state.other_order.remove(&(size, id));
        state.jobs.remove(&id);
        state.stopped.push(id);
        self.wake.notify_all();
        true
    }

    pub fn stop_all(&self) {
        let mut state = self.state();
        state.stopped_all = true;
        let jobs: Vec<(u64, u64, bool)> = state
            .jobs
            .iter()
            .map(|(id, job)| (*id, job.size, job.active))
            .collect();
        for (id, size, active) in jobs {
            if let Some(job) = state.jobs.get_mut(&id)
                && let Some(cancel) = &job.cancel
            {
                cancel.cancel();
            }
            if active {
                continue;
            }
            state.visible_order.remove(&(size, id));
            state.other_order.remove(&(size, id));
            state.jobs.remove(&id);
            state.stopped.push(id);
        }
        self.wake.notify_all();
    }

    pub fn close(&self) {
        let mut state = self.state();
        state.closed = true;
        for job in state.jobs.values() {
            if let Some(cancel) = &job.cancel {
                cancel.cancel();
            }
        }
        state.jobs.clear();
        state.visible_order.clear();
        state.other_order.clear();
        state.stopped.clear();
        self.wake.notify_all();
    }

    #[must_use]
    pub fn progress(&self, id: u64, bytes_done: u64, bytes_total: u64) -> Option<Progress> {
        if bytes_total <= LARGE_FILE {
            return None;
        }
        let mut state = self.state();
        let job = state.jobs.get(&id)?;
        if !job.active {
            return None;
        }
        let now = Instant::now();
        if state
            .last_progress
            .get(&id)
            .is_some_and(|last| now.duration_since(*last) < PROGRESS_INTERVAL)
        {
            return None;
        }
        state.last_progress.insert(id, now);
        Some(Progress {
            id,
            bytes_done,
            bytes_total,
        })
    }
}

fn take_event(state: &mut State, workers: usize) -> Option<CheckEvent> {
    if state.closed || state.paused {
        return None;
    }
    if let Some(id) = state.stopped.pop() {
        return Some(CheckEvent::Stopped(id));
    }
    if state.active >= workers {
        return None;
    }
    let id = pick(state)?;
    let (size, volumes) = {
        let job = state.jobs.get(&id)?;
        (job.size, job.volumes)
    };
    let remaining = state.jobs.len();
    let job = state.jobs.get_mut(&id)?;
    job.active = true;
    state.active = state.active.saturating_add(1);
    let read_hint = if remaining <= 10 {
        ReadHint::Map
    } else {
        ReadHint::Buffered
    };
    let cancel = CancelToken::new();
    job.cancel = Some(cancel.clone());
    if size > LARGE_FILE {
        increment_volumes(state, volumes);
    }
    Some(CheckEvent::Job(Job {
        id,
        read_hint,
        cancel,
    }))
}

fn pick(state: &mut State) -> Option<u64> {
    let visible = state
        .visible_order
        .iter()
        .find(|(_, id)| can_start(state, *id))
        .map(|(_, id)| *id);
    let id = visible.or_else(|| {
        state
            .other_order
            .iter()
            .find(|(_, id)| can_start(state, *id))
            .map(|(_, id)| *id)
    })?;
    let size = state.jobs.get(&id)?.size;
    state.visible_order.remove(&(size, id));
    state.other_order.remove(&(size, id));
    Some(id)
}

fn can_start(state: &State, id: u64) -> bool {
    let Some(job) = state.jobs.get(&id) else {
        return false;
    };
    if job.active {
        return false;
    }
    if job.size <= LARGE_FILE {
        return true;
    }
    [job.volumes.0, job.volumes.1]
        .iter()
        .all(|volume| !state.active_volumes.contains_key(volume))
}

fn increment_volumes(state: &mut State, volumes: (u64, u64)) {
    *state.active_volumes.entry(volumes.0).or_default() += 1;
    if volumes.1 != volumes.0 {
        *state.active_volumes.entry(volumes.1).or_default() += 1;
    }
}

fn decrement_volumes(state: &mut State, volumes: (u64, u64)) {
    decrement_volume(state, volumes.0);
    if volumes.1 != volumes.0 {
        decrement_volume(state, volumes.1);
    }
}

fn decrement_volume(state: &mut State, volume: u64) {
    if let Some(count) = state.active_volumes.get_mut(&volume) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            state.active_volumes.remove(&volume);
        }
    }
}

#[cfg(test)]
mod tests;
