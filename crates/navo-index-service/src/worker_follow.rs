//! Follows one volume's journal for the worker: reads a batch, recognises folders below
//! `$Extend` the index lacks, applies the batch and asks the rebuild state what it requires.

use std::sync::PoisonError;
use std::time::Instant;

use navo_index::ntfs::{NtfsError, Volume};
use navo_index::pipe::CancelToken;
use navo_index::usn::UsnBatch;

use super::super::answer::Shared;
use super::super::batch as journal_batch;
use super::super::follow_state::Updating;
use super::super::rebuild::{Cause, Followed, Rebuilds};
use super::Worker;

impl Worker {
    /// Reads and applies one journal batch; returns the full build it requires, if any.
    /// A failed apply pauses replay and saves, then rebuilds; a read error is returned. A stop
    /// mid-batch blocks saves (the index holds part of the batch), requests no build and returns
    /// `Cancelled`.
    pub(super) fn follow(
        &mut self,
        volume: &mut Volume,
        cancel: &CancelToken,
    ) -> Result<Option<Cause>, NtfsError> {
        let position = self
            .shared
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .index
            .as_ref()
            .map(|index| (index.position, index.pending_renames().is_empty()));
        let Some((position, renames_settled)) = position else {
            return Ok(None);
        };
        // A restore at this cursor would lose a pending rename half, so it is no replay point.
        let start = renames_settled.then_some(position);
        let _updating = Updating::begin(&self.shared);
        let elapsed = self.started.elapsed();
        let batch = match volume.read_journal(position) {
            Ok(batch) => batch,
            Err(error) => {
                let changes = Err(error);
                let followed = Followed { changes, start };
                return self
                    .rebuilds
                    .after_follow_from(&self.shared, followed, elapsed);
            }
        };
        note_metadata_parents(&self.shared, volume, &batch);
        let records = batch.records.len();
        let changed = batch.next_usn != position.next_usn;
        self.moved = changed;
        let stop = || cancel.is_cancelled();
        let unresolved = match journal_batch::apply_batch(&self.shared, batch, stop) {
            Ok(unresolved) => unresolved,
            Err(NtfsError::Cancelled) => {
                self.rebuilds.apply_cancelled();
                return Err(NtfsError::Cancelled);
            }
            Err(error) => return Ok(self.rebuilds.apply_failed(&self.shared, &error, elapsed)),
        };
        update_lag(&self.shared, volume);
        if changed {
            self.clock.batch(self.started.elapsed(), records);
        }
        verify_inexact_replay(&self.shared, &mut self.rebuilds);
        self.rebuilds.after_follow_from(
            &self.shared,
            Followed {
                changes: Ok(unresolved),
                start,
            },
            elapsed,
        )
    }
}

/// Schedules a verification pass when the batch just applied left a replay inexact; the
/// flag is taken, so one inexact replay schedules one pass. A free function, so it borrows
/// only these two fields while the caller's `Updating` guard still borrows `shared`.
fn verify_inexact_replay(shared: &Shared, rebuilds: &mut Rebuilds) {
    let replay_inexact = {
        let mut state = shared.write().unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut state.replay_inexact)
    };
    if replay_inexact {
        rebuilds.schedule_verification(Instant::now(), "replay_cycle");
    }
}

/// Records the batch's unknown parents that the volume confirms below `$Extend`, so a rename
/// into `$Deleted` after a restore resolves without a rebuild. Reads nothing for an empty batch.
fn note_metadata_parents(shared: &Shared, volume: &mut Volume, journal: &UsnBatch) {
    if journal.records.is_empty() {
        return;
    }
    // The published index is dropped before the batch applies, so the apply never copies it.
    let found = journal_batch::published(shared)
        .map(|index| volume.metadata_parents(journal, &index))
        .unwrap_or_default();
    if !found.is_empty() {
        journal_batch::note_metadata_folders(shared, found);
    }
}

/// Queries the journal's current position and stores the byte lag behind the index cursor.
fn update_lag(shared: &Shared, volume: &Volume) {
    let Ok(journal_pos) = volume.journal_position() else {
        return;
    };
    let mut state = shared.write().unwrap_or_else(PoisonError::into_inner);
    let index_next_usn = state
        .index
        .as_ref()
        .map_or(journal_pos.next_usn, |index| index.position.next_usn);
    state.follow.lag_bytes =
        u64::try_from(journal_pos.next_usn.saturating_sub(index_next_usn)).unwrap_or(0);
}
