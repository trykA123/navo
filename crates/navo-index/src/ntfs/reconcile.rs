//! Holds late NTFS records during pass two and resolves them before publication.

use std::collections::{HashMap, HashSet};

use super::super::directory_graph::{DirectoryGraph, DirectoryRecord, UnplacedFolder};
use super::super::skipped::is_extend;
use super::super::{InexactBuild, directory_scan, is_folder};
use super::two_pass::{add_file, add_record, check_cancel};
use super::{
    Enumeration, JournalState, NtfsError, NtfsIndex, SizeHint, VolumeIo, reserve_for_build,
};
use crate::pipe::CancelToken;
use crate::usn::{UsnRecord, parse_batch};

pub(super) struct LateRecords {
    folders: HashMap<u128, DirectoryRecord>,
    files: Vec<UsnRecord>,
    file_limit: usize,
    folder_limit: usize,
    skipped: u64,
    held_cap: bool,
}

struct Reconciled {
    skipped: u64,
    sizes_unknown: u64,
    inexact_reason: Option<InexactBuild>,
}

impl LateRecords {
    /// Holds folders left from pass one under the limits for deferred records.
    fn new(folders: Vec<UnplacedFolder>, file_limit: usize, folder_limit: usize) -> Self {
        let mut late = Self {
            folders: HashMap::new(),
            files: Vec::new(),
            file_limit,
            folder_limit,
            skipped: 0,
            held_cap: false,
        };
        for folder in folders {
            late.hold_folder(folder.record);
        }
        late
    }

    /// Keeps the latest folder record for this file reference.
    pub(super) fn hold_folder(&mut self, record: DirectoryRecord) {
        if self.folders.len() == self.folder_limit && !self.folders.contains_key(&record.file_id) {
            self.skip_over_cap();
            return;
        }
        self.folders.insert(record.file_id, record);
    }

    /// Holds one file for the final graph or counts it when the queue is full.
    pub(super) fn hold_file(&mut self, record: UsnRecord) {
        if self.files.len() < self.file_limit {
            self.files.push(record);
        } else {
            self.skip_over_cap();
        }
    }

    /// Counts one record that cannot be placed in the build.
    pub(super) fn skip(&mut self) {
        self.skipped += 1;
    }

    /// Counts a record dropped by a cap and marks the build as inexact.
    fn skip_over_cap(&mut self) {
        self.held_cap = true;
        self.skip();
    }

    /// Inserts late folders and files, reporting counts and whether verification is needed.
    fn insert_into(
        self,
        volume: &mut impl VolumeIo,
        index: &mut NtfsIndex,
        cancel: &CancelToken,
    ) -> Result<Reconciled, NtfsError> {
        let mut graph = DirectoryGraph::new();
        for folder in self.folders.into_values() {
            check_cancel(cancel)?;
            graph.push(folder)?;
        }
        let resolved = graph.insert_into(index)?;
        let skipped = self.skipped + resolved.skipped + resolved.unplaced.len() as u64;
        let unplaced = resolved.unplaced.iter().map(|folder| folder.record.file_id);
        index.skipped_folders = unplaced.collect();
        index.metadata_folders = metadata_folders(&resolved.unplaced);
        let cycles: HashSet<_> = resolved
            .unplaced
            .into_iter()
            .filter(|folder| folder.cycle)
            .map(|folder| folder.record.file_id)
            .collect();
        let inexact_reason = if self.held_cap || resolved.held_cap {
            Some(InexactBuild::HeldCap)
        } else {
            (!cycles.is_empty()).then_some(InexactBuild::Cycle)
        };
        let mut reconciled = Reconciled {
            skipped,
            sizes_unknown: 0,
            inexact_reason,
        };
        for record in self.files {
            check_cancel(cancel)?;
            if cycles.contains(&record.parent_id) {
                reconciled.inexact_reason.get_or_insert(InexactBuild::Cycle);
                reconciled.skipped += 1;
            } else if is_folder(index, record.file_id) || !is_folder(index, record.parent_id) {
                reconciled.skipped += 1;
            } else if let Some(unknown) = add_file(volume, index, record)? {
                reconciled.sizes_unknown += unknown;
            } else {
                reconciled.skipped += 1;
            }
        }
        Ok(reconciled)
    }
}

/// Lists the unplaced folders below `$Extend` (MFT record 11, which the build never places):
/// its children by their parent reference, then their unplaced subfolders, walked with a stack.
fn metadata_folders(unplaced: &[UnplacedFolder]) -> HashSet<u128> {
    let mut children: HashMap<u128, Vec<u128>> = HashMap::new();
    for folder in unplaced {
        let record = &folder.record;
        children
            .entry(record.parent_id)
            .or_default()
            .push(record.file_id);
    }
    let mut found = HashSet::new();
    let mut stack: Vec<u128> = children
        .iter()
        .filter(|(parent, _)| is_extend(**parent))
        .flat_map(|(_, files)| files.iter().copied())
        .collect();
    while let Some(folder) = stack.pop() {
        if found.insert(folder) {
            stack.extend(children.get(&folder).into_iter().flatten().copied());
        }
    }
    found
}

/// Builds folders and files, reconciles the late records, and checks journal continuity.
#[expect(
    clippy::too_many_arguments,
    reason = "the volume, limit, hint and cancel token are separate build inputs"
)]
pub(super) fn enumerate_with_late_limits<const CAP: usize, const FOLDER_CAP: usize>(
    volume: &mut impl VolumeIo,
    drive: char,
    limit: usize,
    hint: SizeHint,
    cancel: &CancelToken,
) -> Result<Enumeration, NtfsError> {
    let scan = directory_scan::scan_directories(volume, drive, limit, cancel)?;
    let mut index = scan.index;
    let sized = reserve_for_build(&mut index, hint, scan.capacity, drive);
    let mut late = LateRecords::new(scan.late_folders, CAP, FOLDER_CAP);
    late.held_cap |= scan.held_cap;
    let (mut cursor, mut count, mut sizes_unknown) = (0, 0, 0);
    while count < limit {
        check_cancel(cancel)?;
        let Some(bytes) = volume.enumerate(cursor)? else {
            break;
        };
        let batch = parse_batch(&bytes)?;
        let next = batch.next_usn.cast_unsigned();
        if next <= cursor {
            return Err(NtfsError::InvalidResponse(
                "enumeration cursor did not advance",
            ));
        }
        cursor = next;
        for record in batch.records.into_iter().take(limit - count) {
            count += 1;
            sizes_unknown += add_record(volume, &mut index, record, &mut late)?;
        }
    }
    check_cancel(cancel)?;
    let reconciled = late.insert_into(volume, &mut index, cancel)?;
    check_cancel(cancel)?;
    JournalState::parse(&volume.query_journal()?)?.check(index.position, "pass2_continuity")?;
    check_cancel(cancel)?;
    // A sized build keeps its spare until the service has dropped the old index.
    if !sized {
        index.table.trim_slack();
    }
    index.ids.shrink_to_fit();
    Ok(Enumeration {
        index,
        unresolved: Vec::new(),
        sizes_unknown: sizes_unknown + reconciled.sizes_unknown,
        blocked_peak: 0,
        skipped: scan.skipped + reconciled.skipped,
        inexact: reconciled.inexact_reason.is_some(),
        inexact_reason: reconciled.inexact_reason,
    })
}
