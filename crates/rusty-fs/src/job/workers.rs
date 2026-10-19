use crate::{AutoTuning, DriveClass};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CopyMode {
    #[default]
    Auto,
    Standard,
}

impl CopyMode {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Standard => "Standard",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerCounts {
    pub ssd: usize,
    pub network: usize,
    pub slow: usize,
}

impl From<&AutoTuning> for WorkerCounts {
    fn from(tuning: &AutoTuning) -> Self {
        Self {
            ssd: tuning.ssd_workers,
            network: tuning.network_workers,
            slow: tuning.slow_workers,
        }
    }
}

#[must_use]
pub fn copy_workers(source: DriveClass, target: DriveClass, counts: WorkerCounts) -> usize {
    let sides = [source, target];
    let count = if sides
        .iter()
        .any(|side| matches!(side, DriveClass::Hdd | DriveClass::Unknown))
    {
        counts.slow
    } else if sides.contains(&DriveClass::Network) {
        counts.network
    } else {
        counts.ssd
    };
    count.max(1)
}

#[must_use]
pub fn job_workers(
    mode: CopyMode,
    source: DriveClass,
    target: DriveClass,
    counts: WorkerCounts,
) -> usize {
    match mode {
        CopyMode::Auto => copy_workers(source, target, counts),
        CopyMode::Standard => 1,
    }
}
