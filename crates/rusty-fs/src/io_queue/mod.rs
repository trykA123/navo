mod runner;
mod scheduler;

use rusty_core::Location;

pub use runner::{IoQueue, Job, JobSlot, OwnerGuard};
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
        match self {
            Self::Ssd => 8,
            Self::Network => 4,
            Self::Removable => 2,
            Self::Hdd | Self::Unknown => 1,
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
