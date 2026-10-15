use std::sync::{Arc, Condvar, Mutex};

use super::{JobId, OwnerId, Priority, Scheduler, VolumeId, VolumeKind};
use crate::provider::CancelToken;

pub type Job = Box<dyn FnOnce(&CancelToken) + Send + 'static>;

struct Shared {
    scheduler: Mutex<Scheduler>,
    wake: Condvar,
}

pub struct IoQueue {
    shared: Arc<Shared>,
}

pub struct OwnerGuard {
    shared: Arc<Shared>,
    id: OwnerId,
}

impl IoQueue {
    #[must_use]
    pub fn new() -> Self {
        todo!()
    }

    pub fn set_volume(&self, volume: VolumeId, kind: VolumeKind) {
        todo!()
    }

    #[must_use]
    pub fn owner(&self) -> OwnerGuard {
        todo!()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "signature fixed by the P6 F9 contract"
    )]
    pub fn submit(&self, volume: VolumeId, priority: Priority, owner: OwnerId, job: Job) -> JobId {
        todo!()
    }

    pub fn reprioritise(&self, job: JobId, priority: Priority) -> bool {
        todo!()
    }

    pub fn cancel_owner(&self, owner: OwnerId) {
        todo!()
    }

    pub fn shutdown(self) {
        todo!()
    }
}

impl Default for IoQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for IoQueue {
    fn drop(&mut self) {
        todo!()
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
        todo!()
    }
}
