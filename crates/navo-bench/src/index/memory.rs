use serde::{Deserialize, Serialize};

use super::ParseError;

/// Known service memory parts, matched on the service's fixed labels.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MemoryPartKind {
    Names,
    FileIds,
    Tree,
    Totals,
    Journal,
    Sessions,
    Process,
}

impl MemoryPartKind {
    fn from_label(label: &str) -> Option<Self> {
        Some(match label {
            "index service names" => Self::Names,
            "index service file ids" => Self::FileIds,
            "index service tree" => Self::Tree,
            "index service totals" => Self::Totals,
            "index service journal" => Self::Journal,
            "index service pipe sessions" => Self::Sessions,
            "index service private bytes" => Self::Process,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct PartMemory {
    pub part: MemoryPartKind,
    pub bytes: u64,
    pub items: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct VolumeMemory {
    pub slots: u64,
    pub live: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct IndexMemory {
    pub busy: bool,
    pub parts: Vec<PartMemory>,
    pub volumes: Vec<VolumeMemory>,
}

impl IndexMemory {
    pub fn process_bytes(&self) -> Option<u64> {
        self.parts
            .iter()
            .find(|part| part.part == MemoryPartKind::Process)
            .map(|part| part.bytes)
    }
}

#[derive(Deserialize)]
pub(super) struct RawMemory {
    #[serde(default)]
    busy: bool,
    #[serde(default)]
    parts: Vec<RawPart>,
    #[serde(default)]
    volumes: Option<Vec<RawVolume>>,
}

#[derive(Deserialize)]
struct RawPart {
    #[serde(default)]
    name: String,
    #[serde(default)]
    bytes: u64,
    #[serde(default)]
    items: u64,
}

#[derive(Deserialize)]
struct RawVolume {
    #[serde(default)]
    slots: u64,
    #[serde(default)]
    live: u64,
}

impl RawMemory {
    pub(super) fn into_memory(self) -> IndexMemory {
        IndexMemory {
            busy: self.busy,
            parts: self
                .parts
                .iter()
                .filter_map(|raw| {
                    Some(PartMemory {
                        part: MemoryPartKind::from_label(&raw.name)?,
                        bytes: raw.bytes,
                        items: raw.items,
                    })
                })
                .collect(),
            volumes: self
                .volumes
                .unwrap_or_default()
                .iter()
                .map(|raw| VolumeMemory {
                    slots: raw.slots,
                    live: raw.live,
                })
                .collect(),
        }
    }
}

/// Reads one line of `navo --index-memory --json` (tester builds).
pub fn parse_memory(json: &str) -> Result<IndexMemory, ParseError> {
    serde_json::from_str::<RawMemory>(json)
        .map(RawMemory::into_memory)
        .map_err(|_| ParseError::Json)
}
