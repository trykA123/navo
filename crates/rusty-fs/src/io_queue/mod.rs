mod pools;
mod runner;
mod scheduler;

use rusty_core::Location;

use crate::DriveClass;

pub use pools::{JobKind, Pools};
pub use runner::{Classifier, IoQueue, Job, JobSlot, OwnerGuard};
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VolumeId(String);

impl VolumeId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn root(&self) -> Option<Location> {
        if self.0.contains("://") {
            return None;
        }
        let root = if self.0 == "/" {
            self.0.clone()
        } else {
            format!("{}\\", self.0)
        };
        Location::new(root).ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OwnerId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(u64);

#[must_use]
pub fn volume_of(location: &Location) -> VolumeId {
    if !location.is_local() {
        return VolumeId(format!("{}://", location.provider().as_str()));
    }
    VolumeId(local_volume(location.as_str()))
}

fn local_volume(text: &str) -> String {
    const VERBATIM_UNC: &str = r"\\?\UNC\";
    if let Some(rest) = text
        .get(..VERBATIM_UNC.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(VERBATIM_UNC))
        .and_then(|_| text.get(VERBATIM_UNC.len()..))
    {
        return unc_volume(rest);
    }
    let text = text.strip_prefix(r"\\?\").unwrap_or(text);
    if let Some(rest) = text.strip_prefix(r"\\") {
        return unc_volume(rest);
    }
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), Some(':')) if letter.is_ascii_alphabetic() => {
            format!("{}:", letter.to_ascii_uppercase())
        }
        _ => "/".to_owned(),
    }
}

fn unc_volume(rest: &str) -> String {
    let mut parts = rest.split('\\');
    let server = parts.next().unwrap_or_default().to_lowercase();
    let share = parts.next().unwrap_or_default().to_lowercase();
    format!(r"\\{server}\{share}")
}

#[cfg(test)]
mod tests;
