use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rusty_core::{Location, OperationPlan, Step};
use rusty_fs::{
    CancelToken, DriveClass, JobKind, JobSlot, Priority, VolumeId, drive_class, volume_of,
};

use super::work::JobWork;
use super::{Browser, JobStatus, JobUpdate, PendingOp, Run};
use crate::browser::Message;

pub type SlowVolume = Arc<dyn Fn(&Location) -> bool + Send + Sync>;
pub(in crate::browser) type SlowVolumes = HashSet<VolumeId>;

pub(in crate::browser) struct JobQueue {
    slow: SlowVolume,
    waiting: Vec<Waiting>,
    running: HashMap<u64, SlowVolumes>,
}

struct Waiting {
    pending: PendingOp,
    run: Run,
    cancel: CancelToken,
}

impl Default for JobQueue {
    fn default() -> Self {
        Self {
            slow: Arc::new(is_slow),
            waiting: Vec::new(),
            running: HashMap::new(),
        }
    }
}

pub(in crate::browser) fn is_slow(location: &Location) -> bool {
    location.is_local() && slow_class(drive_class(location))
}

pub(in crate::browser) fn slow_class(class: DriveClass) -> bool {
    match class {
        DriveClass::Hdd | DriveClass::Removable => true,
        DriveClass::Unknown => cfg!(windows),
        DriveClass::Ssd | DriveClass::Network => false,
    }
}

pub(super) fn slow_volumes<'a>(
    places: impl Iterator<Item = &'a Location>,
    slow: &SlowVolume,
) -> SlowVolumes {
    places.filter(|place| slow(place)).map(volume_of).collect()
}

fn plan_places(plan: &OperationPlan) -> impl Iterator<Item = &Location> {
    plan.steps
        .iter()
        .flat_map(|step| match step {
            Step::CopyFile { source, target, .. } | Step::Rename { source, target } => {
                [Some(source), Some(target)]
            }
            Step::MakeLink { target_of, at } => [Some(target_of), Some(at)],
            Step::MakeFolder { target } => [Some(target), None],
            Step::Recycle { location }
            | Step::RemoveFile { location }
            | Step::RemoveLink { location }
            | Step::RemoveFolder { location } => [Some(location), None],
        })
        .flatten()
}

impl Browser {
    fn spawn_job(&self, pending: PendingOp, run: Run, cancel: CancelToken) -> Result<(), String> {
        let work = JobWork {
            id: pending.id,
            copying: self.copying,
            source: pending.sources.first().cloned(),
            target: pending.target,
            plan: pending.plan,
            ops: Arc::clone(&self.provider),
            factory: Arc::clone(&self.operations.log),
            retain_deletes: pending.retain_deletes,
            run,
            cancel,
            sender: self.sender.clone(),
            wake: Arc::clone(&self.wake),
        };
        std::thread::Builder::new()
            .name("navo-job".to_owned())
            .spawn(move || work.run())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub fn set_slow_volume(&mut self, slow: SlowVolume) {
        self.job_queue.slow = slow;
    }

    pub(super) fn slow_volume(&self) -> SlowVolume {
        Arc::clone(&self.job_queue.slow)
    }

    pub(super) fn enqueue_job(&mut self, pending: PendingOp, run: Run, cancel: CancelToken) {
        if pending.slow_volumes.is_none() {
            self.classify_job(&pending);
        }
        self.job_queue.waiting.push(Waiting {
            pending,
            run,
            cancel,
        });
        self.start_queued();
    }

    fn classify_job(&mut self, pending: &PendingOp) {
        let pending = pending.clone();
        let slow = self.slow_volume();
        let (sender, wake) = (self.sender.clone(), Arc::clone(&self.wake));
        let slot = JobSlot {
            volume: pending
                .sources
                .first()
                .map_or_else(|| volume_of(self.state.active_tab().location()), volume_of),
            priority: Priority::Visible,
            owner: self.owner.id(),
        };
        self.in_flight += 1;
        self.pools.queue(JobKind::Listing).submit(
            slot,
            Box::new(move |_| {
                let volumes = slow_volumes(
                    pending
                        .sources
                        .iter()
                        .chain(pending.target.iter())
                        .chain(plan_places(&pending.plan)),
                    &slow,
                );
                let _ = sender.send(Message::Job(JobUpdate::Classified {
                    id: pending.id,
                    volumes,
                }));
                wake();
            }),
        );
    }

    pub(super) fn accept_classified(&mut self, id: u64, volumes: SlowVolumes) -> bool {
        if let Some(waiting) = self
            .job_queue
            .waiting
            .iter_mut()
            .find(|job| job.pending.id == id)
        {
            waiting.pending.slow_volumes = Some(volumes);
            self.start_queued();
        }
        true
    }

    pub(super) fn start_queued(&mut self) {
        let mut blocked: SlowVolumes = self.job_queue.running.values().flatten().cloned().collect();
        let mut index = 0;
        while index < self.job_queue.waiting.len() {
            let pending = &self.job_queue.waiting[index].pending;
            let Some(volumes) = pending.slow_volumes.as_ref() else {
                blocked.extend(
                    pending
                        .sources
                        .iter()
                        .chain(pending.target.iter())
                        .chain(plan_places(&pending.plan))
                        .map(volume_of),
                );
                index += 1;
                continue;
            };
            let ready = volumes.is_disjoint(&blocked);
            blocked.extend(volumes.iter().cloned());
            if !ready {
                index += 1;
                continue;
            }
            let Waiting {
                pending,
                run,
                cancel,
            } = self.job_queue.waiting.remove(index);
            let id = pending.id;
            self.job_queue
                .running
                .insert(id, pending.slow_volumes.clone().unwrap_or_default());
            self.invalidate_undo();
            match self.spawn_job(pending, run, cancel) {
                Ok(()) => {
                    self.with_job(id, |job| job.status = JobStatus::Running);
                }
                Err(error) => {
                    self.invalidate_undo();
                    self.job_queue.running.remove(&id);
                    self.with_job(id, |job| job.status = JobStatus::Failed(error));
                    return self.start_queued();
                }
            }
        }
    }

    pub(super) fn release_job(&mut self, id: u64) {
        self.job_queue.running.remove(&id);
        self.start_queued();
    }

    pub(super) fn cancel_queued(&mut self, id: u64) -> bool {
        let Some(index) = self
            .job_queue
            .waiting
            .iter()
            .position(|job| job.pending.id == id)
        else {
            return false;
        };
        self.invalidate_undo();
        self.job_queue.waiting.remove(index);
        self.with_job(id, |job| {
            job.cancel.cancel();
            job.status = JobStatus::Cancelled;
        });
        self.ended.push(id);
        true
    }
}
