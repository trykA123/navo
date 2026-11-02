use serde::{Deserialize, Serialize};

use super::{MemoryPart, MemoryReport, VolumeMemory};

// Unknown part codes preserve the Postcard V1 layout for older clients.
const PROCESS_ID: u16 = 0x8000;
const VOLUME: u16 = 0x8001;

#[derive(Serialize, Deserialize)]
pub(super) struct WireReport {
    parts: Vec<MemoryPart>,
    busy: bool,
}

impl From<MemoryReport> for WireReport {
    fn from(report: MemoryReport) -> Self {
        let mut parts = report.parts;
        if let Some(pid) = report.process_id {
            parts.push(MemoryPart {
                part: PROCESS_ID,
                bytes: u64::from(pid),
                items: 0,
            });
        }
        if let Some(volumes) = report.volumes {
            parts.extend(volumes.into_iter().map(|volume| MemoryPart {
                part: VOLUME,
                bytes: volume.slots,
                items: volume.live,
            }));
        }
        Self {
            parts,
            busy: report.busy,
        }
    }
}

impl From<WireReport> for MemoryReport {
    fn from(wire: WireReport) -> Self {
        let mut report = Self {
            busy: wire.busy,
            ..Self::default()
        };
        for part in wire.parts {
            match part.part {
                PROCESS_ID => report.process_id = u32::try_from(part.bytes).ok(),
                VOLUME => report.volumes.get_or_insert_default().push(VolumeMemory {
                    slots: part.bytes,
                    live: part.items,
                }),
                _ => report.parts.push(part),
            }
        }
        report
    }
}
