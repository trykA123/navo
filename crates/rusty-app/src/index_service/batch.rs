use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

use rusty_index::ntfs::{BatchApply, NtfsError, NtfsIndex, UnresolvedChange};
use rusty_index::usn::UsnBatch;

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

pub(super) fn apply_batch(
    shared: &Shared,
    batch: UsnBatch,
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
        match with_index(shared, |index| step_for(&mut applying, index)) {
            None => return Ok(Vec::new()),
            Some(Err(error)) => return Err(error),
            Some(Ok(true)) => std::thread::sleep(YIELD),
            Some(Ok(false)) => break,
        }
    }
    with_index(shared, |index| applying.finish(index)).unwrap_or_else(|| Ok(Vec::new()))
}

fn step_for(applying: &mut BatchApply, index: &mut NtfsIndex) -> Result<bool, NtfsError> {
    let started = Instant::now();
    while applying.step(index, STEP_CHANGES)? {
        if started.elapsed() >= HOLD {
            return Ok(true);
        }
    }
    Ok(false)
}

fn with_index<T>(shared: &Shared, change: impl FnOnce(&mut NtfsIndex) -> T) -> Option<T> {
    let mut state = shared.write().unwrap_or_else(PoisonError::into_inner);
    state
        .index
        .as_mut()
        .map(|index| change(Arc::make_mut(index)))
}

#[cfg(test)]
#[path = "batch_tests.rs"]
mod tests;
