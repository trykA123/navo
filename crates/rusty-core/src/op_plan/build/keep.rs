use super::{Walk, name_of};
use crate::op_plan::{ItemKind, Resolution, ScannedItem, Step};
use crate::{Location, ProviderPath};

impl Walk<'_> {
    pub(super) fn root_for(&mut self, item: &ScannedItem, target: &Location) -> Option<Location> {
        let Some(base) = self.base else {
            return Some(target.clone());
        };
        let mut root = target.clone();
        for name in relative_folders(base, &item.location) {
            root = root.join(&name).ok()?;
            if !self.ensure_folder(item, &root) {
                return None;
            }
        }
        Some(root)
    }

    fn ensure_folder(&mut self, item: &ScannedItem, folder: &Location) -> bool {
        if self.made.iter().any(|made| made.same_place(folder)) {
            return true;
        }
        let blocked = self.is_planned(folder)
            || self
                .existing_at(folder)
                .is_some_and(|found| found.kind != ItemKind::Folder);
        if blocked {
            self.conflict(item, folder.clone(), Resolution::Skip);
            return false;
        }
        if self.existing_at(folder).is_none() {
            self.steps.push(Step::MakeFolder {
                target: folder.clone(),
            });
        }
        self.made.push(folder.clone());
        true
    }
}

#[must_use]
pub fn relative_folders(base: &Location, location: &Location) -> Vec<ProviderPath> {
    let mut names = Vec::new();
    let mut folder = location.parent();
    while let Some(current) = folder {
        if current.same_place(base) {
            names.reverse();
            return names;
        }
        let Some(name) = name_of(&current) else {
            break;
        };
        names.push(name);
        folder = current.parent();
    }
    Vec::new()
}
