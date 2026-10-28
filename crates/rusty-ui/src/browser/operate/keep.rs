use std::collections::HashMap;

use rusty_core::{ExistingItem, FsError, Location, OpKind, ScannedItem, Tab, relative_folders};
use rusty_fs::{CancelToken, Provider, existing_at};

use super::{OpRequest, Structure};
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
    pub fn toggle_structure(&mut self) -> bool {
        let protected = self.operations.protected.clone();
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        let Some(structure) = pending.structure.as_mut() else {
            return false;
        };
        structure.keep = !structure.keep;
        structure.planning = true;
        let request = OpRequest {
            id: pending.id,
            kind: pending.plan.kind,
            sources: pending.sources.clone(),
            target: pending.target.clone(),
            same_volume: structure.same_volume,
            protected,
            source_tab: pending.source_tab,
            asks: true,
            structure: Some(structure.clone()),
        };
        self.submit_plan(request)
    }

    pub(super) fn is_stale_replan(&self, request: &OpRequest) -> bool {
        let Some(asked) = request.structure.as_ref().filter(|s| s.planning) else {
            return false;
        };
        let current = self
            .pending
            .as_ref()
            .filter(|pending| pending.id == request.id)
            .and_then(|pending| pending.structure.as_ref());
        current.is_none_or(|current| current.keep != asked.keep)
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
