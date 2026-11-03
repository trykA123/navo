use super::MemoryPart;

// Unknown part codes are skipped by older clients; each volume sends all three, BUILDS first.
const BUILDS: u16 = 0x8002;
const LAST_BUILD: u16 = 0x8003;
const DEFERRED: u16 = 0x8004;

/// Why the service last read the whole MFT of a volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildReason {
    SnapshotMissing = 1,
    SnapshotInvalid = 2,
    SnapshotStale = 3,
    JournalReset = 4,
    UnresolvedChanges = 5,
}

const REASONS: [(BuildReason, &str); 5] = [
    (BuildReason::SnapshotMissing, "snapshot_missing"),
    (BuildReason::SnapshotInvalid, "snapshot_invalid"),
    (BuildReason::SnapshotStale, "snapshot_stale"),
    (BuildReason::JournalReset, "journal_reset"),
    (BuildReason::UnresolvedChanges, "unresolved_changes"),
];

impl BuildReason {
    pub fn code(self) -> u64 {
        self as u64
    }

    pub fn from_code(code: u64) -> Option<Self> {
        REASONS
            .iter()
            .find(|(reason, _)| reason.code() == code)
            .map(|(reason, _)| *reason)
    }

    pub fn label(self) -> &'static str {
        REASONS
            .iter()
            .find(|(reason, _)| *reason == self)
            .map_or("unknown", |(_, label)| label)
    }
}

/// Full-build counters of one volume since the service started.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VolumeBuilds {
    pub full_builds: u64,
    pub last_reason: Option<BuildReason>,
    pub last_duration_ms: u64,
    /// Journal batches that left changes unresolved.
    pub unresolved_batches: u64,
    /// Rebuilds postponed by the rebuild window.
    pub deferred_rebuilds: u64,
    /// Attribute or size changes for files the index does not hold.
    pub dropped_changes: u64,
}

pub(super) fn encode(builds: Vec<VolumeBuilds>, parts: &mut Vec<MemoryPart>) {
    for volume in builds {
        parts.extend([
            MemoryPart {
                part: BUILDS,
                bytes: volume.full_builds,
                items: volume.unresolved_batches,
            },
            MemoryPart {
                part: LAST_BUILD,
                bytes: volume.last_duration_ms,
                items: volume.last_reason.map_or(0, BuildReason::code),
            },
            MemoryPart {
                part: DEFERRED,
                bytes: volume.deferred_rebuilds,
                items: volume.dropped_changes,
            },
        ]);
    }
}

/// Folds a build part into `builds`; returns false for any other part.
pub(super) fn decode(part: MemoryPart, builds: &mut Option<Vec<VolumeBuilds>>) -> bool {
    if !matches!(part.part, BUILDS | LAST_BUILD | DEFERRED) {
        return false;
    }
    let volumes = builds.get_or_insert_default();
    if part.part == BUILDS || volumes.is_empty() {
        volumes.push(VolumeBuilds::default());
    }
    let Some(volume) = volumes.last_mut() else {
        return true;
    };
    match part.part {
        BUILDS => {
            volume.full_builds = part.bytes;
            volume.unresolved_batches = part.items;
        }
        LAST_BUILD => {
            volume.last_duration_ms = part.bytes;
            volume.last_reason = BuildReason::from_code(part.items);
        }
        _ => {
            volume.deferred_rebuilds = part.bytes;
            volume.dropped_changes = part.items;
        }
    }
    true
}
