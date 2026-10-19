use crate::DriveClass;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerCounts {
    pub ssd: usize,
    pub network: usize,
    pub slow: usize,
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
