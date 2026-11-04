use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathRole {
    OwnerFolder,
    ScratchRoot,
    CopySource,
    StaleRun,
    OutputFolder,
}

impl fmt::Display for PathRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OwnerFolder => "owner-folder",
            Self::ScratchRoot => "scratch-root",
            Self::CopySource => "copy-source",
            Self::StaleRun => "stale-run",
            Self::OutputFolder => "output-folder",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum GuardError {
    #[error("{0}: invalid-path")]
    InvalidPath(PathRole),
    #[error("{0}: outside-root")]
    OutsideRoot(PathRole),
    #[error("{0}: reparse-point")]
    ReparsePoint(PathRole),
    #[error("{0}: forbidden-parent")]
    ForbiddenParent(PathRole),
    #[error("{0}: owner-overlap")]
    OwnerOverlap(PathRole),
    #[error("{0}: io")]
    Io(PathRole),
    #[error("{0}: entropy")]
    Entropy(PathRole),
    #[error("{0}: clock")]
    Clock(PathRole),
    #[error("{0}: changed")]
    Changed(PathRole),
    #[error("{0}: configuration")]
    Configuration(PathRole),
    #[error("{0}: exists")]
    Exists(PathRole),
    #[error("{0}: missing")]
    Missing(PathRole),
    #[error("{0}: hash-mismatch")]
    Mismatch(PathRole),
    #[error("{0}: marker")]
    Marker(PathRole),
    #[error("{0}: ledger")]
    Ledger(PathRole),
    #[error("{0}: recycle-{1}")]
    Recycle(PathRole, RecycleError),
    /// A new item landed elsewhere and could not be proved to be this run's, so it was left.
    #[error("{0}: stray-left")]
    Left(PathRole),
}

/// Why a Recycle Bin move was refused or did not complete.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RecycleError {
    #[error("not-fixed-drive")]
    NotFixedDrive,
    #[error("permanent-delete")]
    PermanentDelete,
    #[error("aborted")]
    Aborted,
    #[error("other-item")]
    OtherItem,
    #[error("shell")]
    Shell,
    #[error("still-present")]
    StillPresent,
    #[error("io")]
    Io,
}
