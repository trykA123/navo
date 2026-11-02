use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use super::{JobId, Limits, OwnerId, Priority, VolumeId, VolumeKind};

type Slot = (Reverse<Priority>, JobId);

#[derive(Debug)]
struct Volume {
    kind: VolumeKind,
    queued: BTreeSet<Slot>,
    running: usize,
    classified: bool,
}

impl Default for Volume {
    fn default() -> Self {
        Self {
            kind: VolumeKind::Unknown,
            queued: BTreeSet::new(),
            running: 0,
            classified: false,
        }
    }
}

impl Volume {
    fn has_room(&self, limits: Limits) -> bool {
        self.running < limits.limit(self.kind)
    }
}

#[derive(Debug, Clone)]
struct JobInfo {
    volume: VolumeId,
    priority: Priority,
    owner: OwnerId,
    running: bool,
}

#[derive(Debug)]
pub struct Scheduler {
    limits: Limits,
    next_id: u64,
    volumes: BTreeMap<VolumeId, Volume>,
    jobs: BTreeMap<JobId, JobInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cancelled {
    pub queued: Vec<JobId>,
    pub running: Vec<JobId>,
}

impl Scheduler {
    #[must_use]
    pub fn new() -> Self {
        Self::with(Limits::LISTING)
    }

    #[must_use]
    pub fn with(limits: Limits) -> Self {
        Self {
            limits,
            next_id: 0,
            volumes: BTreeMap::new(),
            jobs: BTreeMap::new(),
        }
    }

    pub fn set_volume(&mut self, volume: VolumeId, kind: VolumeKind) {
        let volume = self.volumes.entry(volume).or_default();
        volume.kind = kind;
        volume.classified = true;
    }

    pub(super) fn take_classification(&mut self, volume: &VolumeId) -> bool {
        let Some(volume) = self.volumes.get_mut(volume) else {
            return false;
        };
        !std::mem::replace(&mut volume.classified, true) && volume.kind == VolumeKind::Unknown
    }

    pub fn push(&mut self, volume: VolumeId, priority: Priority, owner: OwnerId) -> JobId {
        let id = JobId(self.next_id);
        self.next_id += 1;
        let slots = self.volumes.entry(volume.clone()).or_default();
        slots.queued.insert((Reverse(priority), id));
        let info = JobInfo {
            volume,
            priority,
            owner,
            running: false,
        };
        self.jobs.insert(id, info);
        id
    }

    pub fn start_next(&mut self) -> Option<(VolumeId, JobId)> {
        let (volume, _) = self
            .volumes
            .iter()
            .filter(|(_, v)| v.has_room(self.limits))
            .filter_map(|(id, v)| v.queued.first().map(|slot| (id, *slot)))
            .min_by_key(|&(_, slot)| slot)?;
        let volume = volume.clone();
        let job = self.start_next_on(&volume)?;
        Some((volume, job))
    }

    pub fn start_next_on(&mut self, volume: &VolumeId) -> Option<JobId> {
        let slots = self.volumes.get_mut(volume)?;
        if !slots.has_room(self.limits) {
            return None;
        }
        let (_, job) = slots.queued.pop_first()?;
        slots.running += 1;
        if let Some(info) = self.jobs.get_mut(&job) {
            info.running = true;
        }
        Some(job)
    }

    pub fn finish(&mut self, job: JobId) -> bool {
        match self.jobs.get(&job) {
            Some(info) if info.running => {}
            _ => return false,
        }
        let Some(info) = self.jobs.remove(&job) else {
            return false;
        };
        if let Some(slots) = self.volumes.get_mut(&info.volume) {
            slots.running = slots.running.saturating_sub(1);
        }
        true
    }

    pub fn reprioritise(&mut self, job: JobId, priority: Priority) -> bool {
        let Some(info) = self.jobs.get_mut(&job).filter(|info| !info.running) else {
            return false;
        };
        let Some(slots) = self.volumes.get_mut(&info.volume) else {
            return false;
        };
        slots.queued.remove(&(Reverse(info.priority), job));
        slots.queued.insert((Reverse(priority), job));
        info.priority = priority;
        true
    }

    pub fn cancel_owner(&mut self, owner: OwnerId) -> Cancelled {
        let mut cancelled = Cancelled::default();
        let owned: Vec<(JobId, JobInfo)> = self
            .jobs
            .iter()
            .filter(|(_, info)| info.owner == owner)
            .map(|(id, info)| (*id, info.clone()))
            .collect();
        for (job, info) in owned {
            if info.running {
                cancelled.running.push(job);
                continue;
            }
            self.jobs.remove(&job);
            if let Some(slots) = self.volumes.get_mut(&info.volume) {
                slots.queued.remove(&(Reverse(info.priority), job));
            }
            cancelled.queued.push(job);
        }
        cancelled
    }

    #[must_use]
    pub fn running(&self, volume: &VolumeId) -> usize {
        self.volumes.get(volume).map_or(0, |v| v.running)
    }

    #[must_use]
    pub fn queued(&self, volume: &VolumeId) -> usize {
        self.volumes.get(volume).map_or(0, |v| v.queued.len())
    }

    pub(super) fn limit(&self, volume: &VolumeId) -> usize {
        let kind = self
            .volumes
            .get(volume)
            .map_or(VolumeKind::Unknown, |v| v.kind);
        self.limits.limit(kind)
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}
