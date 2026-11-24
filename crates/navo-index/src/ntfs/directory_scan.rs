//! Pass-one NTFS scan: retain folders and count file capacity without size I/O.
#![cfg(any(windows, test, feature = "test-support"))]
#![cfg_attr(not(test), allow(dead_code, reason = "activated by T3"))]
use super::directory_graph::{DirectoryGraph, UnplacedFolder};
use super::volume_io::{JournalState, VolumeIo};
use super::{Continuity, Flags, NtfsError, NtfsIndex, SizeHint};
use crate::{pipe::CancelToken, table::TableError};
/// Resolved folders and total build capacities; the index retains the initial journal cursor.
pub(super) struct DirectoryScan {
    pub(super) index: NtfsIndex,
    pub(super) capacity: SizeHint,
    pub(super) skipped: u64,
    pub(super) late_folders: Vec<UnplacedFolder>,
    pub(super) held_cap: bool,
    /// True when `Continuity::AllowWrap` kept a scan the journal outran.
    pub(super) journal_wrapped: bool,
}
/// Scans a record budget into folders/capacities; fails on graph, cancellation or continuity errors
/// (a journal wrap fails only under `Continuity::Strict`).
#[expect(
    clippy::too_many_arguments,
    reason = "the volume, limit, continuity and cancel token are separate build inputs"
)]
pub(super) fn scan_directories(
    volume: &mut impl VolumeIo,
    drive: char,
    limit: usize,
    continuity: Continuity,
    cancel: &CancelToken,
) -> Result<DirectoryScan, NtfsError> {
    let state = JournalState::parse(&volume.query_journal()?)?;
    let mut index = NtfsIndex::new(drive, state.position)?;
    let mut graph = DirectoryGraph::new();
    let (mut cursor, mut count, mut files, mut names) = (0, 0, 0usize, 0usize);
    while count < limit {
        check_cancel(cancel)?;
        let Some(bytes) = volume.enumerate(cursor)? else {
            break;
        };
        let batch = super::parse_batch(&bytes)?;
        let next = batch.next_usn.cast_unsigned();
        if next <= cursor {
            return Err(NtfsError::InvalidResponse("stalled MFT cursor"));
        }
        cursor = next;
        for record in batch.records.into_iter().take(limit - count) {
            count += 1;
            if record.flags.contains(Flags::FOLDER) {
                graph.push(record.into())?;
            } else {
                files = files.checked_add(1).ok_or(TableError::Full)?;
                let bytes = record.name.len() + 1;
                names = names.checked_add(bytes).ok_or(TableError::Full)?;
            }
        }
    }
    check_cancel(cancel)?;
    let resolved = graph.insert_into(&mut index)?;
    let journal_wrapped = JournalState::parse(&volume.query_journal()?)?.check_with(
        state.position,
        "pass1_continuity",
        continuity,
    )?;
    check_cancel(cancel)?;
    let folders = index.table.len();
    let bytes = index.table.live_name_bytes();
    let capacity = SizeHint {
        entries: files.checked_add(folders).ok_or(TableError::Full)?,
        folders,
        name_bytes: names.checked_add(bytes).ok_or(TableError::Full)?,
    };
    Ok(DirectoryScan {
        index,
        capacity,
        skipped: resolved.skipped,
        late_folders: resolved.unplaced,
        held_cap: resolved.held_cap,
        journal_wrapped,
    })
}
/// Rejects cancellation before traversal work and after resolution or continuity I/O.
fn check_cancel(cancel: &CancelToken) -> Result<(), NtfsError> {
    if cancel.is_cancelled() {
        return Err(NtfsError::Cancelled);
    }
    Ok(())
}
#[cfg(test)]
#[path = "directory_scan_edge_tests.rs"]
mod edge_tests;
#[cfg(test)]
#[path = "directory_scan_tests.rs"]
mod tests;
