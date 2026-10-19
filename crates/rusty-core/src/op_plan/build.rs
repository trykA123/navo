use std::collections::HashMap;

use super::names::{key, name_of, numbered, temp_of};
use super::{
    ConflictPolicy, ExistingItem, ItemKind, OpKind, PlanRequest, Resolution, ScannedItem, Step,
};
use crate::{Conflict, Location};

pub(super) struct Walk<'a> {
    request: &'a PlanRequest,
    existing: HashMap<String, &'a ExistingItem>,
    planned: HashMap<String, Vec<Location>>,
    pub(super) steps: Vec<Step>,
    pub(super) conflicts: Vec<Conflict>,
    pub(super) links_skipped: u64,
    pub(super) into_itself: Vec<Location>,
    pub(super) moved: Vec<bool>,
}

impl<'a> Walk<'a> {
    pub(super) fn new(request: &'a PlanRequest) -> Self {
        Self {
            request,
            existing: request
                .existing
                .iter()
                .map(|e| (key(&e.location), e))
                .collect(),
            planned: HashMap::new(),
            steps: Vec::new(),
            conflicts: Vec::new(),
            links_skipped: 0,
            into_itself: Vec::new(),
            moved: vec![false; request.items.len()],
        }
    }

    pub(super) fn copy(&mut self, target: &Location) {
        let mut names = Vec::new();
        let mut skip_below: Option<u32> = None;
        for (index, item) in self.request.items.iter().enumerate() {
            if skip_below.is_some_and(|depth| item.depth > depth) {
                continue;
            }
            skip_below = None;
            if item.depth == 0 && inside_itself(item, target) {
                self.into_itself.push(item.location.clone());
                skip_below = Some(0);
                continue;
            }
            names.truncate(item.depth as usize);
            let Some(name) = name_of(&item.location) else {
                skip_below = Some(item.depth);
                continue;
            };
            names.push(name);
            let Some(destination) = join_all(target, &names) else {
                skip_below = Some(item.depth);
                continue;
            };
            if !self.place(index, item, destination) {
                skip_below = Some(item.depth);
            }
        }
    }

    fn place(&mut self, index: usize, item: &ScannedItem, destination: Location) -> bool {
        match item.kind {
            ItemKind::Link => {
                self.links_skipped += 1;
                true
            }
            ItemKind::Folder => self.place_folder(index, item, destination),
            ItemKind::File => {
                self.place_file(index, item, destination);
                true
            }
        }
    }

    fn place_folder(&mut self, index: usize, item: &ScannedItem, destination: Location) -> bool {
        match self.existing_at(&destination).map(|e| e.kind.clone()) {
            Some(ItemKind::Folder) => {}
            Some(_) => {
                self.conflict(item, destination, Resolution::Skip);
                return false;
            }
            None => {
                self.steps.push(Step::MakeFolder {
                    target: destination,
                });
            }
        }
        self.moved[index] = true;
        true
    }

    fn place_file(&mut self, index: usize, item: &ScannedItem, destination: Location) {
        let same_place = item.location.same_place(&destination);
        let resolution = if same_place {
            Some(self.same_place_resolution(&destination))
        } else {
            self.clash_at(item, &destination)
        };
        let target = match &resolution {
            Some(Resolution::Skip | Resolution::Ask) => None,
            Some(Resolution::Rename(renamed)) => Some(renamed.clone()),
            Some(Resolution::Overwrite) | None => Some(destination.clone()),
        };
        if let Some(resolution) = resolution {
            self.conflict(item, destination, resolution);
        }
        if let Some(target) = target {
            self.note_planned(&target);
            self.steps.push(Step::CopyFile {
                source: item.location.clone(),
                target,
                size: item.size,
            });
            self.moved[index] = true;
        }
    }

    fn same_place_resolution(&self, destination: &Location) -> Resolution {
        match self.request.policy {
            ConflictPolicy::Rename | ConflictPolicy::Ask => self.free_name(destination),
            _ => Resolution::Skip,
        }
    }

    fn resolve(&self, item: &ScannedItem, found: &ExistingItem, target: &Location) -> Resolution {
        match self.request.policy {
            ConflictPolicy::Skip => Resolution::Skip,
            ConflictPolicy::Rename => self.free_name(target),
            ConflictPolicy::OverwriteAll => Resolution::Overwrite,
            ConflictPolicy::Ask => Resolution::Ask,
            ConflictPolicy::OverwriteOlder => match (item.modified_ms, found.modified_ms) {
                (Some(source), Some(existing)) if source > existing => Resolution::Overwrite,
                _ => Resolution::Skip,
            },
        }
    }

    fn free_name(&self, target: &Location) -> Resolution {
        let (Some(folder), Some(name)) = (target.parent(), name_of(target)) else {
            return Resolution::Skip;
        };
        (2..u32::MAX)
            .filter_map(|number| folder.join(&numbered(&name, number)).ok())
            .find(|candidate| {
                self.existing_at(candidate).is_none() && !self.clashes_with_planned(candidate)
            })
            .map_or(Resolution::Skip, Resolution::Rename)
    }

    fn clash_at(&self, item: &ScannedItem, destination: &Location) -> Option<Resolution> {
        if self.clashes_with_planned(destination) {
            return Some(self.planned_resolution(destination));
        }
        self.existing_at(destination)
            .map(|found| self.resolve(item, found, destination))
    }

    fn planned_resolution(&self, destination: &Location) -> Resolution {
        match self.request.policy {
            ConflictPolicy::Rename => self.free_name(destination),
            ConflictPolicy::Ask => Resolution::Ask,
            ConflictPolicy::Skip
            | ConflictPolicy::OverwriteOlder
            | ConflictPolicy::OverwriteAll => Resolution::Skip,
        }
    }

    fn is_planned(&self, location: &Location) -> bool {
        self.planned
            .get(&key(location))
            .is_some_and(|found| found.iter().any(|p| p.same_place(location)))
    }

    fn clashes_with_planned(&self, location: &Location) -> bool {
        self.is_planned(location) || temp_of(location).is_some_and(|temp| self.is_planned(&temp))
    }

    fn note_planned(&mut self, location: &Location) {
        self.planned
            .entry(key(location))
            .or_default()
            .push(location.clone());
    }

    fn existing_at(&self, location: &Location) -> Option<&'a ExistingItem> {
        self.existing
            .get(&key(location))
            .copied()
            .filter(|found| found.location.same_place(location))
    }

    fn conflict(&mut self, item: &ScannedItem, target: Location, resolution: Resolution) {
        self.conflicts.push(Conflict {
            source: item.location.clone(),
            target,
            resolution,
        });
    }

    pub(super) fn remove_moved(&mut self) {
        let items = &self.request.items;
        let mut kept_below: Vec<bool> = Vec::new();
        let mut removals = Vec::new();
        for (index, item) in items.iter().enumerate().rev() {
            let depth = item.depth as usize;
            if kept_below.len() <= depth + 1 {
                kept_below.resize(depth + 2, false);
            }
            let children_kept = std::mem::take(&mut kept_below[depth + 1]);
            let removable = self.moved[index] && !(item.kind == ItemKind::Folder && children_kept);
            kept_below[depth] |= !removable;
            if removable {
                removals.push(removal(item));
            }
        }
        self.steps.extend(removals);
    }
}

pub(super) fn removal(item: &ScannedItem) -> Step {
    let location = item.location.clone();
    match item.kind {
        ItemKind::File => Step::RemoveFile { location },
        ItemKind::Link => Step::RemoveLink { location },
        ItemKind::Folder => Step::RemoveFolder { location },
    }
}

pub(super) fn inside_itself(item: &ScannedItem, target: &Location) -> bool {
    item.kind == ItemKind::Folder
        && std::iter::successors(Some(target.clone()), Location::parent)
            .any(|ancestor| ancestor.same_place(&item.location))
}

fn join_all(target: &Location, names: &[crate::ProviderPath]) -> Option<Location> {
    names
        .iter()
        .try_fold(target.clone(), |folder, name| folder.join(name).ok())
}

pub(super) fn is_copy_like(kind: &OpKind) -> bool {
    matches!(kind, OpKind::Copy | OpKind::Move)
}

impl Walk<'_> {
    pub(super) fn rename_tops(&mut self, target: &Location) {
        for (index, item) in self.request.items.iter().enumerate() {
            if item.depth != 0 {
                continue;
            }
            if inside_itself(item, target) {
                self.into_itself.push(item.location.clone());
                continue;
            }
            let Some(destination) = name_of(&item.location).and_then(|n| target.join(&n).ok())
            else {
                continue;
            };
            if item.location.same_place(&destination) {
                continue;
            }
            let resolution = self.clash_at(item, &destination);
            let renamed = match &resolution {
                Some(Resolution::Skip | Resolution::Ask) => None,
                Some(Resolution::Rename(other)) => Some(other.clone()),
                Some(Resolution::Overwrite) | None => Some(destination.clone()),
            };
            if let Some(resolution) = resolution {
                self.conflict(item, destination, resolution);
            }
            if let Some(renamed) = renamed {
                self.note_planned(&renamed);
                self.steps.push(Step::Rename {
                    source: item.location.clone(),
                    target: renamed,
                });
                self.moved[index] = true;
            }
        }
    }
}
