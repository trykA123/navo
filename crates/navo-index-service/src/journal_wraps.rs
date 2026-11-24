//! Counts consecutive full builds that the USN journal outran, so the third attempt may keep a
//! wrapped build (AF12). Part of the per-volume rebuild policy in `rebuild.rs`.

use navo_index::ntfs::{Continuity, NtfsError};

/// Strict builds that fail on journal continuity before a build may allow a wrap.
const STRICT_ATTEMPTS: u32 = 2;

/// The streak of regular builds that failed with `NtfsError::Reenumerate`.
#[derive(Default)]
pub(super) struct JournalWraps(u32);

impl JournalWraps {
    /// Extends the streak on a continuity failure, ends it on any other failure; a cancel keeps it.
    pub(super) fn failed(&mut self, error: &NtfsError) {
        match error {
            NtfsError::Reenumerate(_) => self.0 = self.0.saturating_add(1),
            NtfsError::Cancelled => {}
            _ => self.0 = 0,
        }
    }

    /// Ends the streak after a build finished.
    pub(super) fn succeeded(&mut self) {
        self.0 = 0;
    }

    /// Returns `AllowWrap` once `STRICT_ATTEMPTS` strict builds in a row lost the journal.
    pub(super) fn continuity(&self) -> Continuity {
        if self.0 >= STRICT_ATTEMPTS {
            Continuity::AllowWrap
        } else {
            Continuity::Strict
        }
    }
}
