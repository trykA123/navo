//! Creates a missing USN journal once on a volume the service indexes (ERROR_JOURNAL_NOT_ACTIVE).
//! The worker calls it after opening a selected volume; the OS calls sit behind `JournalControl`.

use navo_index::ntfs::NtfsError;

use super::rebuild::error_kind;
use super::service_log::Log;

/// `ERROR_JOURNAL_NOT_ACTIVE`: the volume has no USN journal (a freshly formatted volume).
const JOURNAL_NOT_ACTIVE: i32 = 1179;
/// Journal size limit in bytes; the `fsutil usn createjournal m=33554432` default.
pub(super) const MAXIMUM_SIZE: u64 = 32 * 1024 * 1024;
/// Journal growth step in bytes; the `fsutil usn createjournal a=8388608` default.
pub(super) const ALLOCATION_DELTA: u64 = 8 * 1024 * 1024;

/// The two journal calls the decision needs; the Windows volume implements them.
pub(super) trait JournalControl {
    /// Queries the volume's USN journal; fails with the OS error, as 1179 when it is not active.
    fn query_journal(&mut self) -> Result<(), NtfsError>;
    /// Creates the USN journal with the given size limit and growth step, in bytes.
    fn create_journal(&mut self, maximum_size: u64, allocation_delta: u64)
    -> Result<(), NtfsError>;
}

/// What one check did, for the caller's tests; the worker goes on with its normal flow either way.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum JournalCheck {
    /// The journal answered, or failed with an error the normal flow reports.
    Untouched,
    /// The journal was missing and is now created.
    Created,
    /// The journal was missing and the create failed; the normal backoff follows.
    CreateFailed,
    /// The journal is missing again after the one create attempt; the normal backoff follows.
    AlreadyTried,
}

/// Remembers whether this worker already tried to create its volume's journal.
#[derive(Default)]
pub(super) struct JournalCreate {
    tried: bool,
}

impl JournalCreate {
    /// Creates the journal when the query fails with 1179, at most once per worker; logs one line
    /// for the create or its failure. Every other query result leaves the normal flow untouched.
    pub fn ensure(
        &mut self,
        volume: &mut impl JournalControl,
        letter: char,
        log: &Log,
    ) -> JournalCheck {
        match volume.query_journal() {
            Err(NtfsError::Io { source, .. })
                if source.raw_os_error() == Some(JOURNAL_NOT_ACTIVE) => {}
            _ => return JournalCheck::Untouched,
        }
        if self.tried {
            return JournalCheck::AlreadyTried;
        }
        self.tried = true;
        match volume.create_journal(MAXIMUM_SIZE, ALLOCATION_DELTA) {
            Ok(()) => {
                tracing::info!(drive = %letter, "usn journal created");
                log.line(format_args!("usn journal created {letter}:"));
                JournalCheck::Created
            }
            Err(error) => {
                let kind = error_kind(&error);
                tracing::warn!(drive = %letter, %kind, "usn journal create failed");
                log.line(format_args!("usn journal create failed {letter}: {kind}"));
                JournalCheck::CreateFailed
            }
        }
    }
}

#[cfg(windows)]
impl JournalControl for navo_index::ntfs::Volume {
    fn query_journal(&mut self) -> Result<(), NtfsError> {
        self.query_journal_status()
    }

    fn create_journal(
        &mut self,
        maximum_size: u64,
        allocation_delta: u64,
    ) -> Result<(), NtfsError> {
        navo_index::ntfs::Volume::create_journal(self, maximum_size, allocation_delta)
    }
}
