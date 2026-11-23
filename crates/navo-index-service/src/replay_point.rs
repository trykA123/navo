//! Picks the journal cursor a snapshot may carry while a full rebuild waits for unresolved
//! changes, so a restore replays those changes instead of losing them.

#[cfg(test)]
use std::time::Duration;

use navo_index::ntfs::{NtfsError, UnresolvedChange};
use navo_index::usn::JournalPosition;

#[cfg(test)]
use super::super::answer::Shared;
#[cfg(test)]
use super::Cause;
use super::Rebuilds;

impl Rebuilds {
    /// Like [`Self::after_follow_from`] for a batch whose start allows no replay cursor.
    #[cfg(test)]
    pub fn after_follow(
        &mut self,
        shared: &Shared,
        followed: Result<Vec<UnresolvedChange>, NtfsError>,
        now: Duration,
    ) -> Result<Option<Cause>, NtfsError> {
        let start = None;
        self.after_follow_from(
            shared,
            Followed {
                changes: followed,
                start,
            },
            now,
        )
    }

    /// Picks the cursor a due save may write; replay-pausing causes and pending verifications skip.
    pub fn save_cursor(&self) -> SaveCursor {
        if self.journal_lost() || self.verification.is_pending() {
            return SaveCursor::Skip;
        }
        self.replay.cursor(self.is_deferred())
    }

    /// Forgets the replay point once any built or restored index is published.
    pub fn index_published(&mut self) {
        self.replay.published();
    }
}

/// One journal read's outcome and the cursor before it (`None` when a rename half was pending).
pub(crate) struct Followed {
    pub changes: Result<Vec<UnresolvedChange>, NtfsError>,
    pub start: Option<JournalPosition>,
}

/// Which cursor a due snapshot save writes, or whether it must not save at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveCursor {
    /// The index holds every journal change up to its own cursor.
    Live,
    /// Save the live table with this earlier cursor: replay starts before the first unresolved change.
    From(JournalPosition),
    /// No safe cursor exists; keep the older snapshot on disk.
    Skip,
}

/// The cursor before the first batch that left changes unresolved since the last published index.
/// `Some(None)` means that batch began with half of a rename pending, so no cursor is safe.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ReplayPoint(Option<Option<JournalPosition>>);

impl ReplayPoint {
    /// Keeps the start of the first unresolved batch; `start` is `None` when renames were pending.
    pub fn unresolved_batch(&mut self, start: Option<JournalPosition>) {
        self.0.get_or_insert(start);
    }

    /// Forgets the point once a built or restored index is published: its own cursor is exact.
    pub fn published(&mut self) {
        self.0 = None;
    }

    /// Returns the save cursor; `rebuild_waits` is true while a deferred build is queued.
    pub fn cursor(&self, rebuild_waits: bool) -> SaveCursor {
        match self.0 {
            None if rebuild_waits => SaveCursor::Skip,
            None => SaveCursor::Live,
            Some(Some(start)) => SaveCursor::From(start),
            Some(None) => SaveCursor::Skip,
        }
    }
}

/// Returns the cursor to write for an index at `live`, or `None` (skip the save) when the replay
/// point is not on the same journal at or before the live cursor.
pub(crate) fn checked(live: JournalPosition, cursor: SaveCursor) -> Option<JournalPosition> {
    match cursor {
        SaveCursor::Live => Some(live),
        SaveCursor::From(start)
            if start.journal_id == live.journal_id && start.next_usn <= live.next_usn =>
        {
            Some(start)
        }
        SaveCursor::From(_) | SaveCursor::Skip => None,
    }
}
