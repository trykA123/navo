use super::timings::{TimingFolder, TimingStep};
use crate::index::{MemoryPartKind, ServiceState};
use crate::machine::{DefenderState, DiskKind, FreeSpace};

pub(super) fn service(state: ServiceState) -> &'static str {
    match state {
        ServiceState::Running => "running",
        ServiceState::NotRunning => "not running",
        ServiceState::NotInstalled => "not installed",
        ServiceState::Unknown => "unknown",
    }
}

pub(super) fn memory_part(part: MemoryPartKind) -> &'static str {
    match part {
        MemoryPartKind::Names => "names",
        MemoryPartKind::FileIds => "file ids",
        MemoryPartKind::Tree => "tree",
        MemoryPartKind::Totals => "totals",
        MemoryPartKind::Journal => "journal",
        MemoryPartKind::Sessions => "pipe sessions",
        MemoryPartKind::Process => "process private bytes",
    }
}

pub(super) fn disk(kind: DiskKind) -> &'static str {
    match kind {
        DiskKind::Ssd => "SSD",
        DiskKind::Hdd => "HDD",
        DiskKind::Unknown => "unknown",
    }
}

pub(super) fn defender(state: DefenderState) -> &'static str {
    match state {
        DefenderState::On => "real-time protection on",
        DefenderState::Off => "real-time protection off",
        DefenderState::Unknown => "unknown",
    }
}

pub(super) fn free_space(space: FreeSpace) -> &'static str {
    match space {
        FreeSpace::Under10Gib => "under 10 GiB",
        FreeSpace::From10To50Gib => "10 to 50 GiB",
        FreeSpace::From50To200Gib => "50 to 200 GiB",
        FreeSpace::Over200Gib => "over 200 GiB",
        FreeSpace::Unknown => "unknown",
    }
}

pub(super) fn folder(folder: TimingFolder) -> &'static str {
    match folder {
        TimingFolder::Chosen => "your folder",
        TimingFolder::Generated100k => "generated 100k folder",
    }
}

pub(super) fn step(step: TimingStep) -> &'static str {
    match step {
        TimingStep::List => "list (headless)",
        TimingStep::Sort => "sort (headless)",
        TimingStep::FilterKey => "filter keystroke (headless)",
        TimingStep::WindowFirstUsable => "window, first usable",
        TimingStep::KeyToFrame => "window, key to frame",
    }
}
