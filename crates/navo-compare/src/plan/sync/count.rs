use std::collections::HashSet;

use navo_core::{Location, Step};

use super::super::{Side, join_all, segments};
use super::parts::{Candidate, Change};
use super::{SyncBlock, SyncRequest};
use crate::model::{EntryKind, SideEntry};

/// Running step count per part, folders counted as `copy_part` keeps them.
pub(super) struct StepCount<'a> {
    present: &'a HashSet<Location>,
    made: HashSet<Location>,
    copy: usize,
    delete: usize,
    cap: usize,
}

impl<'a> StepCount<'a> {
    pub(super) fn new(present: &'a HashSet<Location>, cap: usize) -> Self {
        Self {
            present,
            made: HashSet::new(),
            copy: 0,
            delete: 0,
            cap,
        }
    }

    pub(super) fn add(&mut self, candidate: &Candidate) -> Result<(), SyncBlock> {
        let steps = match candidate.change {
            Change::Delete => {
                self.delete += candidate.steps.len();
                self.delete
            }
            Change::Copy(_) => {
                let (present, made) = (self.present, &mut self.made);
                self.copy += candidate
                    .steps
                    .iter()
                    .filter(|step| kept(step, present, made))
                    .count();
                self.copy
            }
        };
        if steps > self.cap {
            Err(SyncBlock::TooManySteps { steps })
        } else {
            Ok(())
        }
    }
}

/// A `MakeFolder` is kept once, and only for a folder the rows do not show (Y1-D12).
pub(super) fn kept(step: &Step, present: &HashSet<Location>, made: &mut HashSet<Location>) -> bool {
    match step {
        Step::MakeFolder { target } => !present.contains(target) && made.insert(target.clone()),
        _ => true,
    }
}

/// Folder locations the rows show on the side they hang on; making them again is skipped.
pub(super) fn present_folders(request: &SyncRequest<'_>) -> HashSet<Location> {
    let sides = request.sides;
    request
        .rows
        .iter()
        .flat_map(|row| {
            [
                (&sides.left, row.left.as_deref()),
                (&sides.right, row.right.as_deref()),
            ]
        })
        .filter_map(|(side, entry)| folder_location(side, entry?))
        .collect()
}

fn folder_location(side: &Side, entry: &SideEntry) -> Option<Location> {
    if entry.kind != EntryKind::Folder {
        return None;
    }
    segments(&entry.rel)
        .and_then(|names| join_all(&side.root, &names))
        .ok()
}
