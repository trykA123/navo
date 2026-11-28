//! Wire types and validation for `StatusV2`: per-volume state, lag, deferral and build counters.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::frames::{FolderTotalsReply, Hit, MemoryReport, Response, Status, VolumeStatus};

/// Per-volume index health reported by `Response::StatusV2`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VolumeState {
    #[default]
    Ready,
    Building,
    Updating,
    Behind,
    RebuildWaiting,
    Stale,
}

impl VolumeState {
    /// Encodes the state as its stable wire byte.
    pub const fn to_code(self) -> u8 {
        match self {
            Self::Ready => 0,
            Self::Building => 1,
            Self::Updating => 2,
            Self::Behind => 3,
            Self::RebuildWaiting => 4,
            Self::Stale => 5,
        }
    }

    /// Decodes a wire byte; unknown future codes map safely to `Updating`.
    pub const fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Ready,
            1 => Self::Building,
            3 => Self::Behind,
            4 => Self::RebuildWaiting,
            5 => Self::Stale,
            _ => Self::Updating,
        }
    }
}

impl Serialize for VolumeState {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.to_code())
    }
}

impl<'de> Deserialize<'de> for VolumeState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        u8::deserialize(deserializer).map(Self::from_code)
    }
}

/// Extended per-volume status carried alongside `VolumeStatus` in `Response::StatusV2`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeDetail {
    pub state: VolumeState,
    pub total_files: Option<u64>,
    pub lag_bytes: u64,
    pub rebuild_in_s: Option<u32>,
    pub full_builds: u64,
    pub last_full_build_age_s: Option<u64>,
}

impl VolumeDetail {
    /// Derives a minimal detail record from a plain `VolumeStatus` when no richer detail was supplied.
    pub fn from_status(status: &VolumeStatus) -> Self {
        let state = if status.has_blocking_error() {
            VolumeState::Stale
        } else {
            match status.progress {
                None => VolumeState::Ready,
                Some(100..) => VolumeState::Updating,
                Some(_) => VolumeState::Building,
            }
        };
        Self {
            state,
            total_files: status.total_files,
            ..Self::default()
        }
    }
}

/// Raw wire mirror of `Response` that validates `StatusV2` invariants on decode.
#[derive(Deserialize)]
pub(super) enum WireResponse {
    Hits {
        generation: u64,
        hits: Vec<Hit>,
    },
    Status {
        generation: u64,
        status: Status,
    },
    FolderTotalsV1 {
        generation: u64,
        reply: FolderTotalsReply,
    },
    MemoryV1 {
        generation: u64,
        report: MemoryReport,
    },
    StatusV1 {
        generation: u64,
        #[serde(with = "super::status_wire")]
        status: Status,
    },
    VersionV1 {
        generation: u64,
        revision: u32,
        build: String,
    },
    StatusV2 {
        generation: u64,
        status: Status,
        detail: Vec<VolumeDetail>,
    },
}

impl TryFrom<WireResponse> for Response {
    type Error = &'static str;

    fn try_from(wire: WireResponse) -> Result<Self, Self::Error> {
        match wire {
            WireResponse::Hits { generation, hits } => Ok(Self::Hits { generation, hits }),
            WireResponse::Status { generation, status } => Ok(Self::Status { generation, status }),
            WireResponse::FolderTotalsV1 { generation, reply } => {
                Ok(Self::FolderTotalsV1 { generation, reply })
            }
            WireResponse::MemoryV1 { generation, report } => {
                Ok(Self::MemoryV1 { generation, report })
            }
            WireResponse::StatusV1 { generation, status } => {
                Ok(Self::StatusV1 { generation, status })
            }
            WireResponse::VersionV1 {
                generation,
                revision,
                build,
            } => Ok(Self::VersionV1 {
                generation,
                revision,
                build,
            }),
            WireResponse::StatusV2 {
                generation,
                mut status,
                mut detail,
            } => {
                if detail.len() != status.volumes.len() {
                    return Err("status detail count does not match its volumes");
                }
                for (volume, item) in status.volumes.iter_mut().zip(&mut detail) {
                    volume.total_files = item.total_files;
                    volume.clamp_total();
                    item.total_files = volume.total_files;
                }
                Ok(Self::StatusV2 {
                    generation,
                    status,
                    detail,
                })
            }
        }
    }
}

#[cfg(test)]
#[path = "status_v2_tests.rs"]
mod tests;
