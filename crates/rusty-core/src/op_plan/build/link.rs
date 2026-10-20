use super::{Walk, name_of};
use crate::Location;
use crate::op_plan::names::shortcut_name;
use crate::op_plan::{Resolution, Step};

impl Walk<'_> {
    pub(in crate::op_plan) fn link_tops(&mut self, target: &Location) {
        let request = self.request;
        for item in request.items.iter().filter(|item| item.depth == 0) {
            let Some(destination) = name_of(&item.location)
                .and_then(|name| target.join(&shortcut_name(&name, target)).ok())
            else {
                continue;
            };
            let taken = item.location.same_place(&destination)
                || self.clashes_with_planned(&destination)
                || self.existing_at(&destination).is_some();
            let at = if taken {
                let resolution = self.free_name(&destination);
                let renamed = match &resolution {
                    Resolution::Rename(renamed) => Some(renamed.clone()),
                    _ => None,
                };
                self.conflict(item, destination, resolution);
                renamed
            } else {
                Some(destination)
            };
            if let Some(at) = at {
                self.note_planned(&at);
                self.steps.push(Step::MakeLink {
                    target_of: item.location.clone(),
                    at,
                });
            }
        }
    }
}
