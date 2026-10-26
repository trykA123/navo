use std::io;
use std::path::PathBuf;

mod quarantine;
mod salvage;

pub use quarantine::{acknowledge_quarantine, quarantine_database};
pub use salvage::salvage_database;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum SalvageOutcome {
    Copied { rows: u64 },
    Reset { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SalvagedTable {
    pub name: &'static str,
    pub outcome: SalvageOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UnrecoveredJob {
    pub id: Option<i64>,
    pub kind: Option<String>,
    pub created_at: Option<i64>,
    pub plan: Option<Vec<u8>>,
    pub held: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct UnrecoveredRetained {
    pub id: Option<i64>,
    pub record: Option<Vec<u8>>,
    pub target: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SalvageReport {
    pub tables: Vec<SalvagedTable>,
    pub jobs: Vec<UnrecoveredJob>,
    pub retained: Vec<UnrecoveredRetained>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct QuarantineReport {
    pub slot: u64,
    pub database: Option<PathBuf>,
    pub wal: Option<PathBuf>,
    pub shm: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error("{operation} {}: {source}", path.display())]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("database recovery is already running for {}", .0.display())]
    Busy(PathBuf),
    #[error("invalid recovery record at {}", .0.display())]
    InvalidRecord(PathBuf),
    #[error("both {} and {} exist; keep both for manual recovery", original.display(), target.display())]
    AmbiguousMove { original: PathBuf, target: PathBuf },
    #[error("{} must be a regular file", .0.display())]
    NotFile(PathBuf),
    #[error("writing recovered data: {0}")]
    Destination(#[from] rusqlite::Error),
    #[error("reading recovery source {}: {source}", path.display())]
    Source {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },
    #[error("recovery destination contains rows in {0}")]
    DestinationNotEmpty(&'static str),
    #[error("recovery source version {found} is newer than supported version {supported}")]
    UnsupportedVersion { found: u32, supported: u32 },
}

fn io_error(operation: &'static str, path: &std::path::Path, source: io::Error) -> RecoveryError {
    RecoveryError::Io {
        operation,
        path: path.to_owned(),
        source,
    }
}
