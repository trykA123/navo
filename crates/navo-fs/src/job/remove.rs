use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, PoisonError};

use navo_core::{FsError, FsErrorKind, Location, OpKind, OperationPlan, Step};

use super::StepOutcome;
use crate::{CopiedSource, EntryKind, ProviderOps};

pub(super) struct Removals<'p> {
    kind: OpKind,
    folders: HashSet<&'p Location>,
    /// Each move source as its copy started, so its removal can tell whether it changed since.
    copied: Mutex<HashMap<Location, CopiedSource>>,
}

impl<'p> Removals<'p> {
    pub(super) fn of(plan: &'p OperationPlan) -> Self {
        let folders = match plan.kind {
            OpKind::Delete => plan
                .steps
                .iter()
                .filter_map(|step| match step {
                    Step::RemoveFolder { location } => Some(location),
                    _ => None,
                })
                .collect(),
            _ => HashSet::new(),
        };
        Self {
            kind: plan.kind,
            folders,
            copied: Mutex::new(HashMap::new()),
        }
    }

    /// Records a move source's file ID, size and modified time as its copy starts.
    pub(super) fn note_copy_start(
        &self,
        ops: &dyn ProviderOps,
        source: &Location,
    ) -> Result<(), FsError> {
        if self.kind != OpKind::Move {
            return Ok(());
        }
        let entry = ops.metadata(source)?;
        if entry.kind != EntryKind::File {
            return Ok(());
        }
        let seen = CopiedSource {
            identity: ops.file_identity(source)?,
            size: entry.size,
            modified_ms: entry.modified_ms,
        };
        self.copied().insert(source.clone(), seen);
        Ok(())
    }

    /// Removes a file; a move source whose copy this run made is removed only while unchanged.
    fn remove_file(&self, ops: &dyn ProviderOps, location: &Location) -> Result<(), FsError> {
        let seen = self.copied().get(location).copied();
        match seen {
            Some(expected) => ops.remove_copied_source(location, expected)?,
            None => ops.remove_file(location)?,
        }
        self.copied().remove(location);
        Ok(())
    }

    fn copied(&self) -> std::sync::MutexGuard<'_, HashMap<Location, CopiedSource>> {
        self.copied.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub(super) fn remove(
    ops: &dyn ProviderOps,
    removals: &Removals<'_>,
    step: &Step,
) -> Result<StepOutcome, FsError> {
    let (result, location) = match step {
        Step::RemoveFile { location } => (removals.remove_file(ops, location), location),
        Step::RemoveLink { location } => (ops.remove_link(location), location),
        Step::RemoveFolder { location } => (ops.remove_empty_folder(location), location),
        _ => return Err(FsError::new(FsErrorKind::Unknown)),
    };
    match result {
        Ok(()) => Ok(StepOutcome::Done),
        Err(error)
            if removals.kind == OpKind::Delete
                && error.kind == FsErrorKind::NotFound
                && already_gone(ops, removals, location) =>
        {
            Ok(StepOutcome::Skipped)
        }
        Err(error) => Err(error),
    }
}

fn already_gone(ops: &dyn ProviderOps, removals: &Removals<'_>, location: &Location) -> bool {
    if !matches!(ops.metadata(location), Err(error) if error.kind == FsErrorKind::NotFound) {
        return false;
    }
    for ancestor in std::iter::successors(location.parent(), Location::parent) {
        let planned = removals.folders.contains(&ancestor);
        match ops.metadata(&ancestor) {
            Ok(_) => return true,
            Err(error) if planned && error.kind == FsErrorKind::NotFound => {}
            Err(_) => return false,
        }
    }
    false
}
