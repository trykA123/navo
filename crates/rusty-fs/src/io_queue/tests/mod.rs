use super::*;

mod props;
mod runner;
mod scheduler;
mod volume;

const A: OwnerId = OwnerId(1);
const B: OwnerId = OwnerId(2);

fn vol(name: &str) -> VolumeId {
    VolumeId(name.to_owned())
}

fn limit_of(kind: VolumeKind) -> usize {
    match kind {
        VolumeKind::Ssd => 8,
        VolumeKind::Hdd | VolumeKind::Unknown => 1,
        VolumeKind::Network => 4,
        VolumeKind::Removable => 2,
    }
}

fn scheduler_with(volume: &str, kind: VolumeKind) -> Scheduler {
    let mut scheduler = Scheduler::new();
    scheduler.set_volume(vol(volume), kind);
    scheduler
}
