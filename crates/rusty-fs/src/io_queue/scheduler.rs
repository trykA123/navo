use super::{JobId, OwnerId, Priority, VolumeId, VolumeKind};

#[derive(Debug, Default)]
pub struct Scheduler {}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cancelled {
    pub queued: Vec<JobId>,
    pub running: Vec<JobId>,
}

impl Scheduler {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_volume(&mut self, volume: VolumeId, kind: VolumeKind) {
        todo!()
    }

    pub fn push(&mut self, volume: VolumeId, priority: Priority, owner: OwnerId) -> JobId {
        todo!()
    }

    pub fn start_next(&mut self) -> Option<(VolumeId, JobId)> {
        todo!()
    }

    pub fn start_next_on(&mut self, volume: &VolumeId) -> Option<JobId> {
        todo!()
    }

    pub fn finish(&mut self, job: JobId) -> bool {
        todo!()
    }

    pub fn reprioritise(&mut self, job: JobId, priority: Priority) -> bool {
        todo!()
    }

    pub fn cancel_owner(&mut self, owner: OwnerId) -> Cancelled {
        todo!()
    }

    #[must_use]
    pub fn running(&self, volume: &VolumeId) -> usize {
        todo!()
    }

    #[must_use]
    pub fn queued(&self, volume: &VolumeId) -> usize {
        todo!()
    }
}
