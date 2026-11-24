mod abort_win;
mod pools;
mod runner;
mod scheduler;

use crate::DriveClass;

pub use pools::{JobKind, Pools};
pub use runner::{Classifier, IoQueue, Job, JobSlot, OwnerGuard};
pub use scheduler::{Cancelled, Scheduler};
pub use sidewise_fs::{VolumeId, volume_of};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    Idle,
    Prefetch,
    Visible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VolumeKind {
    Ssd,
    Hdd,
    Network,
    Removable,
    Unknown,
}

impl VolumeKind {
    #[must_use]
    pub fn limit(self) -> usize {
        Limits::LISTING.limit(self)
    }
}

impl From<DriveClass> for VolumeKind {
    fn from(class: DriveClass) -> Self {
        match class {
            DriveClass::Ssd => Self::Ssd,
            DriveClass::Hdd => Self::Hdd,
            DriveClass::Network => Self::Network,
            DriveClass::Removable => Self::Removable,
            DriveClass::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub ssd: usize,
    pub hdd: usize,
    pub network: usize,
    pub removable: usize,
}

impl Limits {
    pub const LISTING: Self = Self {
        ssd: 8,
        hdd: 1,
        network: 4,
        removable: 2,
    };

    pub const BACKGROUND: Self = Self {
        ssd: 1,
        hdd: 1,
        network: 1,
        removable: 1,
    };

    #[must_use]
    pub fn limit(self, kind: VolumeKind) -> usize {
        match kind {
            VolumeKind::Ssd => self.ssd,
            VolumeKind::Network => self.network,
            VolumeKind::Removable => self.removable,
            VolumeKind::Hdd | VolumeKind::Unknown => self.hdd,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OwnerId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(u64);

#[cfg(test)]
mod tests;
