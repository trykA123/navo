use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, PoisonError};

use navo_core::{FsError, FsErrorKind, Location, OpKind, OperationPlan, Step};

use super::StepOutcome;
use crate::{CopiedSource, CopyControl, MoveCopy, ProviderOps};

pub(super) struct Removals<'p> {
    kind: OpKind,
    folders: HashSet<&'p Location>,
    /// Each move source as its copy read it, so its removal can tell whether it changed since.
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

    /// True for a move: its copies must prove their sources before the sources are removed.
    pub(super) fn moves(&self) -> bool {
        self.kind == OpKind::Move
    }

    /// Copies a move's source and keeps what the copy read as the proof its removal needs.
    pub(super) fn copy_source(
        &self,
        ops: &dyn ProviderOps,
        request: MoveCopy<'_>,
        control: CopyControl<'_>,
    ) -> Result<u64, FsError> {
        let source = request.source.clone();
        self.copied().remove(&source);
        let (written, read) = ops.copy_moved_file(request, control)?;
        if let Some(read) = read.filter(CopiedSource::proves) {
            self.copied().insert(source, read);
        }
        Ok(written)
    }

    /// Removes a file. A move removes a source only through the proof its copy left in this run;
    /// without one (a resumed job, a volume that proves nothing) it keeps the source.
    fn remove_file(&self, ops: &dyn ProviderOps, location: &Location) -> Result<(), FsError> {
        if !self.moves() {
            return ops.remove_file(location);
        }
        let seen = self.copied().get(location).copied();
        let Some(expected) = seen else {
            return Err(FsError {
                location: Some(location.clone()),
                ..FsError::new(FsErrorKind::IdentityUnavailable)
            });
        };
        ops.remove_copied_source(location, expected)?;
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
