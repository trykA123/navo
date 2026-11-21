//! The errors of the job writer (J2): `WriteError` for one item write, `JournalError` for the
//! journal itself, and the map from an OS error to the item error the UI names (NC147).

use std::fmt;
use std::io;

use navo_core::FsErrorKind;

/// Why one item write failed. Each failed write rolls its whole job back first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// The target has no write permission (`compare-copy-read-only`).
    ReadOnly,
    /// The target is on a network drive (`compare-copy-network`).
    Network,
    /// The target changed on disk between the check and the replace (ChangedOnDisk).
    Changed,
    /// Another process holds the target open (`compare-copy-in-use`).
    InUse,
    /// The disk is full (`compare-copy-failed` with the reason).
    Full,
    /// Any other failure, with its reason.
    Failed(String),
    /// The write failed for `cause`, and `failed` items of the job could not be rolled back
    /// now: those targets may hold the new bytes until recovery at the next start (NC145).
    NotRolledBack { cause: Box<WriteError>, failed: u32 },
}

impl fmt::Display for WriteError {
    /// A short English reason; the UI maps the variant to its own l10n id.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadOnly => f.write_str("the file is read-only"),
            Self::Network => f.write_str("the file is on a network drive"),
            Self::Changed => f.write_str("the file changed on disk"),
            Self::InUse => f.write_str("the file is in use"),
            Self::Full => f.write_str("the disk is full"),
            Self::Failed(reason) => f.write_str(reason),
            Self::NotRolledBack { cause, failed } => write!(
                f,
                "{cause}; {failed} file(s) not rolled back yet, the next start rolls them back"
            ),
        }
    }
}

impl std::error::Error for WriteError {}

/// Why the journal or a job's log failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalError {
    /// A file step failed; the text names the step and the OS error.
    Io(String),
    /// The `Commit` line was written but its sync failed (NC162), or a rollback was left to the
    /// next start: the targets keep
    /// the new bytes and recovery at the next start settles the job from the log. Never
    /// undoable now.
    InDoubt(String),
}

impl fmt::Display for JournalError {
    /// The step and the OS error.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(reason) => write!(f, "journal: {reason}"),
            Self::InDoubt(reason) => write!(
                f,
                "journal: commit in doubt, the next start settles it: {reason}"
            ),
        }
    }
}

impl std::error::Error for JournalError {}

/// Builds a `JournalError::Io` that names the failed step.
pub(crate) fn journal_io(step: &str, error: &io::Error) -> JournalError {
    JournalError::Io(format!("{step}: {error}"))
}

/// Maps an OS error of step `step` to the item error: sharing and lock errors are `InUse`,
/// a full disk or quota is `Full`, a read-only volume is `ReadOnly`, the rest is `Failed`.
pub(crate) fn classify(step: &str, error: &io::Error) -> WriteError {
    if let Some(code) = error.raw_os_error() {
        match os_kind(code) {
            FsErrorKind::SharingViolation | FsErrorKind::Locked => return WriteError::InUse,
            FsErrorKind::DiskFull => return WriteError::Full,
            _ => {}
        }
    }
    match error.kind() {
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => WriteError::Full,
        io::ErrorKind::ReadOnlyFilesystem => WriteError::ReadOnly,
        io::ErrorKind::ResourceBusy | io::ErrorKind::ExecutableFileBusy => WriteError::InUse,
        _ => WriteError::Failed(format!("{step}: {error}")),
    }
}

/// The file system error kind of a raw OS code on this platform.
fn os_kind(code: i32) -> FsErrorKind {
    #[cfg(windows)]
    {
        FsErrorKind::from_windows_code(code.cast_unsigned())
    }
    #[cfg(not(windows))]
    {
        FsErrorKind::from_errno(code)
    }
}
