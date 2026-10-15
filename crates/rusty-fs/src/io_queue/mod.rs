mod runner;
mod scheduler;

use rusty_core::Location;

pub use runner::{IoQueue, Job, OwnerGuard};
pub use scheduler::{Cancelled, Scheduler};

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
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VolumeId(String);

impl VolumeId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OwnerId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(u64);

#[must_use]
pub fn volume_of(location: &Location) -> VolumeId {
    todo!()
}

#[cfg(test)]
mod tests;
