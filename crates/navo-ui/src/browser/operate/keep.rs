use std::collections::HashMap;

use navo_core::{
    ConflictPolicy, ExistingItem, FsError, Location, OpKind, ScannedItem, Tab, relative_folders,
};
use navo_fs::{CancelToken, Provider, existing_at};

use super::{OpRequest, Replan, Structure};
use crate::browser::Browser;

pub(super) fn selected_names(tab: &Tab) -> Vec<&str> {
    let Some(listing) = tab.listing() else {
        return Vec::new();
    };
    let marked = tab.selection().marked();
    let rows = if marked.is_empty() {
        vec![tab.view().cursor]
    } else {
        marked
    };
    rows.into_iter()
        .filter_map(|row| listing.get(row))
        .map(|entry| entry.name.as_str())
        .collect()
}

pub(super) fn structure_for(tab: &Tab, kind: OpKind, same_volume: bool) -> Option<Structure> {
    let offered = matches!(kind, OpKind::Copy | OpKind::Move)
        && crate::browser::find::is_results(tab.location())
        && selected_names(tab)
            .iter()
            .any(|name| name.contains(['\\', '/']));
    offered.then(|| Structure {
        base: tab.base_or_location().clone(),
        keep: true,
        planning: false,
        same_volume,
    })
}

impl Browser {
    /// How many selected rows have no safe location (an archive entry such as `../evil.txt`),
    /// so `selected_sources` leaves them out of F5, a drag or the clipboard; 0 when none.
    #[must_use]
    pub fn skipped_sources(&self) -> usize {
        let tab = self.state.active_tab();
        let names = selected_names(tab);
        names
            .into_iter()
            .filter(|name| tab.entry_location(name).is_err())
            .count()
    }

    /// Flips "Keep folder structure" and rebuilds the pending plan with the picked conflict policy;
    /// false when no pending plan offers the choice.
    pub fn toggle_structure(&mut self) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        let Some(structure) = pending.structure.as_mut() else {
            return false;
        };
        structure.keep = !structure.keep;
        structure.planning = true;
        self.replan()
    }

    /// Rebuilds the pending plan so names already at the target follow `policy`; false when
    /// nothing is pending or `policy` is already picked. An overwrite pick is the user's call:
    /// the dialog warns when Undo could not restore the old file.
    pub fn pick_policy(&mut self, policy: ConflictPolicy) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        if pending.replan.policy == policy {
            return false;
        }
        pending.replan.policy = policy;
        self.replan()
    }

    /// Sends the pending op to the planning job again, with its structure and picked policy.
    fn replan(&mut self) -> bool {
        let protected = self.operations.protected.clone();
        let Some(pending) = self.pending.as_ref() else {
            return false;
        };
        let structure = pending.structure.clone();
        let request = OpRequest {
            id: pending.id,
            kind: pending.plan.kind,
            sources: pending.sources.clone(),
            target: pending.target.clone(),
            same_volume: structure
                .as_ref()
                .map_or(pending.replan.same_volume, |s| s.same_volume),
            protected,
            source_tab: pending.source_tab,
            asks: true,
            structure,
            stream: pending.stream.clone(),
            policy: pending.replan.policy.clone(),
            results: pending.replan.results.clone(),
        };
        self.submit_plan(request)
    }

    /// True when `request` rebuilds a shown plan and the dialog has moved on: it was closed, or
    /// the structure or conflict policy now picked differs from the one the request asked for.
    pub(super) fn is_stale_replan(&self, request: &OpRequest) -> bool {
        if !is_replan(request) {
            return false;
        }
        let Some(current) = self.pending.as_ref().filter(|p| p.id == request.id) else {
            return true;
        };
        let keep = |structure: Option<&Structure>| structure.map(|s| s.keep);
        let structure_moved = keep(request.structure.as_ref()) != keep(current.structure.as_ref());
        structure_moved || current.replan.policy != request.policy
    }
}

/// True when `request` rebuilds a plan the dialog shows: a structure flip, or a picked policy
/// (first plans always ask).
pub(super) fn is_replan(request: &OpRequest) -> bool {
    request.structure.as_ref().is_some_and(|s| s.planning) || request.policy != ConflictPolicy::Ask
}

/// The replan record of a plan built for `request`: its policy is both picked and planned.
pub(super) fn replan_of(request: &OpRequest) -> Replan {
    Replan {
        policy: request.policy.clone(),
        planned: request.policy.clone(),
        same_volume: request.same_volume,
        results: request.results.clone(),
    }
}

pub(super) fn existing_kept(
    provider: &dyn Provider,
    items: &[ScannedItem],
    (target, base): (&Location, &Location),
    cancel: &CancelToken,
) -> Result<Vec<ExistingItem>, FsError> {
    let mut levels: Vec<Location> = Vec::new();
    let mut groups: Groups = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut current = None;
    for item in items {
        if item.depth == 0 {
            let root = root_of(target, base, &item.location, &mut levels);
            current = root.map(|root| group_slot(&mut groups, &mut index, root));
        }
        if let Some((_, group)) = current.and_then(|slot| groups.get_mut(slot)) {
            group.push(item.clone());
        }
    }
    let mut existing = Vec::new();
    for level in &levels {
        existing.extend(existing_at(provider, &[], level, cancel)?);
    }
    for (root, group) in &groups {
        existing.extend(existing_at(provider, group, root, cancel)?);
    }
    existing.sort_by(|a, b| a.location.as_str().cmp(b.location.as_str()));
    existing.dedup_by(|a, b| a.location == b.location);
    Ok(existing)
}

type Groups = Vec<(Location, Vec<ScannedItem>)>;

fn group_slot(groups: &mut Groups, index: &mut HashMap<String, usize>, root: Location) -> usize {
    let next = groups.len();
    let slot = *index.entry(root.as_str().to_owned()).or_insert(next);
    if slot == next {
        groups.push((root, Vec::new()));
    }
    slot
}

fn root_of(
    target: &Location,
    base: &Location,
    location: &Location,
    levels: &mut Vec<Location>,
) -> Option<Location> {
    let mut root = target.clone();
    for name in relative_folders(base, location) {
        if !levels.contains(&root) {
            levels.push(root.clone());
        }
        root = root.join(&name).ok()?;
    }
    Some(root)
}
