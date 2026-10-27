use std::collections::HashSet;

use rusty_core::{FsError, FsErrorKind, Location, OpKind, OperationPlan, Step};

use super::StepOutcome;
use crate::ProviderOps;

pub(super) struct Removals<'p> {
    kind: OpKind,
    folders: HashSet<&'p Location>,
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
        }
    }
}

pub(super) fn remove(
    ops: &dyn ProviderOps,
    removals: &Removals<'_>,
    step: &Step,
) -> Result<StepOutcome, FsError> {
    let (result, location) = match step {
        Step::RemoveFile { location } => (ops.remove_file(location), location),
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
