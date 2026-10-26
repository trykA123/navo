use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};

use super::{JobId, OwnerId, Priority, Scheduler, VolumeId, VolumeKind};
use crate::provider::CancelToken;

pub type Job = Box<dyn FnOnce(&CancelToken) + Send + 'static>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobSlot {
    pub volume: VolumeId,
    pub priority: Priority,
    pub owner: OwnerId,
}

#[derive(Default)]
struct State {
    scheduler: Scheduler,
    jobs: HashMap<JobId, Job>,
    tokens: HashMap<JobId, CancelToken>,
    workers: HashMap<VolumeId, usize>,
    handles: Vec<JoinHandle<()>>,
    closed: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    idle: Condvar,
    next_owner: AtomicU64,
}

pub struct IoQueue {
    shared: Arc<Shared>,
}

pub struct OwnerGuard {
    shared: Arc<Shared>,
    id: OwnerId,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn cancel_owner(&self, owner: OwnerId) {
        let dropped: Vec<Job> = {
            let mut state = self.lock();
            let cancelled = state.scheduler.cancel_owner(owner);
            cancelled
                .running
                .iter()
                .filter_map(|job| state.tokens.get(job))
                .for_each(CancelToken::cancel);
            cancelled
                .queued
                .iter()
                .filter_map(|job| state.jobs.remove(job))
                .collect()
        };
        drop(dropped);
    }
}

fn spawn_workers(shared: &Arc<Shared>, state: &mut State, volume: &VolumeId) {
    let limit = state.scheduler.limit(volume);
    let wanted = state.scheduler.queued(volume);
    let live = state.workers.get(volume).copied().unwrap_or(0);
    let room = limit.saturating_sub(live).min(wanted);
    state.handles.retain(|handle| !handle.is_finished());
    for _ in 0..room {
        let (worker_shared, worker_volume) = (Arc::clone(shared), volume.clone());
        let spawned = thread::Builder::new()
            .name("navo-io".to_owned())
            .spawn(move || work(&worker_shared, &worker_volume));
        let Ok(handle) = spawned else {
            break;
        };
        state.handles.push(handle);
        *state.workers.entry(volume.clone()).or_default() += 1;
    }
}

fn work(shared: &Shared, volume: &VolumeId) {
    loop {
        let mut state = shared.lock();
        let next = if state.closed {
            None
        } else {
            state.scheduler.start_next_on(volume)
        };
        let Some(id) = next else {
            if let Some(count) = state.workers.get_mut(volume) {
                *count = count.saturating_sub(1);
            }
            shared.idle.notify_all();
            return;
        };
        let token = CancelToken::new();
        state.tokens.insert(id, token.clone());
        let job = state.jobs.remove(&id);
        drop(state);
        if let Some(job) = job {
            let _ = catch_unwind(AssertUnwindSafe(|| job(&token)));
        }
        let mut state = shared.lock();
        state.tokens.remove(&id);
        state.scheduler.finish(id);
    }
}

impl IoQueue {
    #[must_use]
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared::default()),
        }
    }

    pub fn set_volume(&self, volume: VolumeId, kind: VolumeKind) {
        let mut state = self.shared.lock();
        state.scheduler.set_volume(volume.clone(), kind);
        if !state.closed {
            spawn_workers(&self.shared, &mut state, &volume);
        }
    }

    #[must_use]
    pub fn owner(&self) -> OwnerGuard {
        let id = self.shared.next_owner.fetch_add(1, Ordering::Relaxed);
        OwnerGuard {
            shared: Arc::clone(&self.shared),
            id: OwnerId(u64::MAX - id),
        }
    }

    pub fn submit(&self, slot: JobSlot, job: Job) -> JobId {
        let JobSlot {
            volume,
            priority,
            owner,
        } = slot;
        let mut state = self.shared.lock();
        let id = state.scheduler.push(volume.clone(), priority, owner);
        if state.closed {
            state.scheduler.cancel_owner(owner);
            return id;
        }
        state.jobs.insert(id, job);
        spawn_workers(&self.shared, &mut state, &volume);
        id
    }

    pub fn reprioritise(&self, job: JobId, priority: Priority) -> bool {
        self.shared.lock().scheduler.reprioritise(job, priority)
    }

    pub fn cancel_owner(&self, owner: OwnerId) {
        self.shared.cancel_owner(owner);
    }

    pub fn shutdown(self) {
        drop(self);
    }

    fn close(&self) {
        let (dropped, handles) = {
            let mut state = self.shared.lock();
            state.closed = true;
            state.tokens.values().for_each(CancelToken::cancel);
            let dropped: Vec<Job> = state.jobs.drain().map(|(_, job)| job).collect();
            (dropped, std::mem::take(&mut state.handles))
        };
        drop(dropped);
        let me = thread::current().id();
        for handle in handles {
            if handle.thread().id() != me {
                let _ = handle.join();
            }
        }
    }
}

impl Default for IoQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for IoQueue {
    fn drop(&mut self) {
        self.close();
    }
}

impl OwnerGuard {
    #[must_use]
    pub fn id(&self) -> OwnerId {
        self.id
    }
}

impl Drop for OwnerGuard {
    fn drop(&mut self) {
        self.shared.cancel_owner(self.id);
    }
}
