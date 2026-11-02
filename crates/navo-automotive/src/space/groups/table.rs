use std::collections::HashMap;

use super::{Child, Group};
use crate::space::{Kind, Owner};

#[derive(Default)]
pub(super) struct Table {
    index: HashMap<String, usize>,
    groups: Vec<(Group, HashMap<String, usize>)>,
}

impl Table {
    pub(super) fn add(&mut self, (name, listed): (&str, bool), owner: &Owner, child: &str) {
        let at = self.slot(name, listed);
        let (group, children) = &mut self.groups[at];
        group.bytes = group.bytes.saturating_add(owner.size);
        group.items += 1;
        let bytes = &mut group.by_kind[owner.kind.index()];
        *bytes = bytes.saturating_add(owner.size);
        if owner.image {
            group.image = group.image.saturating_add(owner.size);
        }
        let place = *children
            .entry(child.to_ascii_lowercase())
            .or_insert_with(|| {
                group.children.push(Child {
                    name: child.to_owned(),
                    bytes: 0,
                    by_kind: [0; Kind::COUNT],
                });
                group.children.len() - 1
            });
        let kid = &mut group.children[place];
        kid.bytes = kid.bytes.saturating_add(owner.size);
        let kid_bytes = &mut kid.by_kind[owner.kind.index()];
        *kid_bytes = kid_bytes.saturating_add(owner.size);
    }

    fn slot(&mut self, name: &str, listed: bool) -> usize {
        let groups = &mut self.groups;
        *self
            .index
            .entry(name.to_ascii_lowercase())
            .or_insert_with(|| {
                groups.push((
                    Group {
                        name: name.to_owned(),
                        by_kind: [0; Kind::COUNT],
                        bytes: 0,
                        image: 0,
                        items: 0,
                        listed,
                        children: Vec::new(),
                    },
                    HashMap::new(),
                ));
                groups.len() - 1
            })
    }

    pub(super) fn finish(self) -> Vec<Group> {
        let mut groups: Vec<Group> = self
            .groups
            .into_iter()
            .map(|(mut group, _)| {
                group
                    .children
                    .sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
                group
            })
            .collect();
        groups.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
        groups
    }
}
