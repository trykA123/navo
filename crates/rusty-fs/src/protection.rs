use std::collections::HashSet;

use rusty_core::{FsError, Location, OperationPlan, Step};

use crate::{CancelToken, EntryKind, ProviderOps, mutation_location, scan};

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
