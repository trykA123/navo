use std::collections::HashSet;
use std::ops::ControlFlow;

use rusty_core::{FsError, FsErrorKind, Location, OperationPlan, Step};

use crate::{CancelToken, EntryBatch, EntryKind, ProviderOps, mutation_location, scan};

pub fn check_mutation(
    plan: &OperationPlan,
    ops: &dyn ProviderOps,
    cancel: &CancelToken,
) -> Result<(), FsError> {
    for path in plan.steps.iter().flat_map(mutated) {
        mutation_location(path)?;
    }
    let candidates: HashSet<_> = plan
        .steps
        .iter()
        .flat_map(|step| match step {
            Step::Rename { source, target } => vec![source.clone(), target.clone()],
            Step::Recycle { location } | Step::RemoveFolder { location } => vec![location.clone()],
            _ => vec![],
        })
        .collect();
    for source in &candidates {
        if std::iter::successors(source.parent(), Location::parent)
            .any(|parent| candidates.contains(&parent))
        {
            continue;
        }
        match ops.metadata(source) {
            Ok(entry) if entry.kind == EntryKind::Folder => (),
            Ok(_) => continue,
            Err(error) if error.kind == rusty_core::FsErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }
        for item in scan(ops, std::slice::from_ref(source), cancel)? {
            crate::local::refuse_reserved(&item.location)?;
        }
    }
    Ok(())
}

pub fn check_relocation(
    plan: &OperationPlan,
    ops: &dyn ProviderOps,
    cancel: &CancelToken,
) -> Result<(), FsError> {
    let sources: HashSet<_> = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            Step::Rename { source, .. } => Some(source),
            Step::Recycle { location } => Some(location),
            _ => None,
        })
        .collect();
    for source in &sources {
        if std::iter::successors(source.parent(), Location::parent)
            .any(|parent| sources.contains(&parent))
        {
            continue;
        }
        match ops.metadata(source) {
            Ok(entry) if entry.kind == EntryKind::Folder => {
                refuse_reserved_below(source, ops, cancel)?
            }
            Ok(_) => (),
            Err(error) if error.kind == FsErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn refuse_reserved_below(
    root: &Location,
    ops: &dyn ProviderOps,
    cancel: &CancelToken,
) -> Result<(), FsError> {
    let mut pending = vec![root.clone()];
    while let Some(folder) = pending.pop() {
        if cancel.is_cancelled() {
            return Err(FsError {
                location: Some(folder),
                ..FsError::new(FsErrorKind::Cancelled)
            });
        }
        let mut refused = Ok(());
        let mut sink =
            |batch: EntryBatch| reserved_child(&folder, batch, &mut pending, &mut refused);
        ops.list(&folder, &mut sink, cancel)?;
        refused?;
    }
    Ok(())
}

fn reserved_child(
    folder: &Location,
    batch: EntryBatch,
    pending: &mut Vec<Location>,
    refused: &mut Result<(), FsError>,
) -> ControlFlow<()> {
    for entry in batch.entries {
        let Ok(child) = folder.join(&entry.name) else {
            continue;
        };
        if let Err(error) = crate::local::refuse_reserved(&child) {
            *refused = Err(error);
            return ControlFlow::Break(());
        }
        if entry.kind == EntryKind::Folder {
            pending.push(child);
        }
    }
    ControlFlow::Continue(())
}

pub(crate) fn mutated(step: &Step) -> Vec<&Location> {
    match step {
        Step::Rename { source, target } => vec![source, target],
        Step::CopyFile { target, .. } | Step::MakeFolder { target } => vec![target],
        Step::MakeLink { at, .. } => vec![at],
        Step::Recycle { location }
        | Step::RemoveFile { location }
        | Step::RemoveLink { location }
        | Step::RemoveFolder { location } => vec![location],
    }
}

#[cfg(test)]
mod tests;
