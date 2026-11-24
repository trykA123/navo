//! Applies journal changes to the published index and records dropped changes.

use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

use navo_index::ntfs::{BatchApply, NtfsError, NtfsIndex, UnresolvedChange};
use navo_index::usn::UsnBatch;

use super::answer::Shared;

const STEP_CHANGES: usize = 256;
const HOLD: Duration = Duration::from_millis(5);
const YIELD: Duration = Duration::from_millis(3);

pub(super) fn published(shared: &Shared) -> Option<Arc<NtfsIndex>> {
    shared
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .index
        .clone()
}

/// Applies `batch` to the published index in short write slices and returns unresolved changes.
/// `stop` is polled before each slice; a stop returns `Cancelled` with the cursor unmoved, and the
/// index may hold part of the batch, so the caller must never save it.
pub(super) fn apply_batch(
    shared: &Shared,
    batch: UsnBatch,
    stop: impl Fn() -> bool,
) -> Result<Vec<UnresolvedChange>, NtfsError> {
    apply_in_slices(shared, batch, stop, |started, _| started.elapsed() >= HOLD)
}

/// Like [`apply_batch`]; `slice_full(slice start, steps taken)` ends a write slice, so tests can
/// cut slices by step count instead of by the wall clock.
fn apply_in_slices(
    shared: &Shared,
    batch: UsnBatch,
    stop: impl Fn() -> bool,
    slice_full: impl Fn(Instant, usize) -> bool,
) -> Result<Vec<UnresolvedChange>, NtfsError> {
    let journal = shared
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .index
        .as_ref()
        .map(|index| (index.position, index.pending_renames().clone()));
    let Some((position, renames)) = journal else {
        return Ok(Vec::new());
    };
    let mut applying = BatchApply::new(batch, position, renames)?;
    loop {
        if stop() {
            return Err(NtfsError::Cancelled);
        }
        match with_index(shared, |index| step_for(&mut applying, index, &slice_full)) {
            None => return Ok(Vec::new()),
            Some(Err(error)) => return Err(error),
            Some(Ok(true)) => std::thread::sleep(YIELD),
            Some(Ok(false)) => break,
        }
    }
    let Some(result) = with_index(shared, |index| applying.finish_with_cycle_skips(index)) else {
        return Ok(Vec::new());
    };
    let (unresolved, cycle_skips) = result?;
    if cycle_skips != 0 {
        let mut state = shared.write().unwrap_or_else(PoisonError::into_inner);
        state.builds.dropped_changes += cycle_skips;
        state.replay_inexact = true;
    }
    Ok(unresolved)
}

/// Records folders a journal read confirmed below `$Extend` in the published index, before the
/// batch that names them applies; does nothing without an index.
pub(super) fn note_metadata_folders(shared: &Shared, folders: Vec<u128>) {
    with_index(shared, |index| index.note_metadata_folders(folders));
}

/// Applies steps until `slice_full` says the slice is over; returns true while steps remain.
fn step_for(
    applying: &mut BatchApply,
    index: &mut NtfsIndex,
    slice_full: impl Fn(Instant, usize) -> bool,
) -> Result<bool, NtfsError> {
    let started = Instant::now();
    let mut steps = 0;
    while applying.step(index, STEP_CHANGES)? {
        steps += 1;
        if slice_full(started, steps) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Runs `change` on the published index under the write lock, copying it when a reader still
/// holds it; drops the cached profile ids first, since any change may move a profile folder.
fn with_index<T>(shared: &Shared, change: impl FnOnce(&mut NtfsIndex) -> T) -> Option<T> {
    let mut state = shared.write().unwrap_or_else(PoisonError::into_inner);
    state.search.forget_profile_ids();
    let index = state.index.as_mut()?;
    let index = match Arc::get_mut(index) {
        Some(index) => index,
        None => {
            tracing::warn!(
                table_bytes = index.table.memory_bytes(),
                id_bytes = index.ids.memory_bytes(),
                "a reader still holds the index; the batch copied it"
            );
            Arc::make_mut(index)
        }
    };
    Some(change(index))
}

#[cfg(test)]
#[path = "batch_tests.rs"]
mod tests;
