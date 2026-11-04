use std::num::NonZeroU32;

use super::{NAME_END, NO_PARENT, NameTable};

/// Old table id to new id. Stores `new + 1`, so a removed slot needs no reserved id.
pub struct Remap(Vec<Option<NonZeroU32>>);

impl Remap {
    /// New id for `old`; `None` when that id was removed or lies beyond the old table.
    pub fn get(&self, old: u32) -> Option<u32> {
        self.0
            .get(old as usize)
            .copied()
            .flatten()
            .map(|new| new.get() - 1)
    }
}

/// Copies the kept slots of `column` into an exact `Vec`, mapping each value.
fn kept<T: Copy, U>(column: &[T], map: &Remap, live: usize, value: impl Fn(T) -> U) -> Vec<U> {
    let mut kept = Vec::with_capacity(live);
    kept.extend(
        (0u32..)
            .zip(column)
            .filter(|&(id, _)| map.get(id).is_some())
            .map(|(_, &item)| value(item)),
    );
    kept
}

impl NameTable {
    /// Drops removed slots column by column, so only one old column is copied at a time.
    pub fn compact(&mut self) -> Remap {
        let slots = self.slots();
        let mut map = Remap(vec![None; slots as usize]);
        let mut next = 0u32;
        for id in 0..slots {
            if self.is_live(id) {
                map.0[id as usize] = next.checked_add(1).and_then(NonZeroU32::new);
                next += 1;
            }
        }
        let live = next as usize;
        (self.names, self.offsets) = self.kept_names(&map, live);
        self.flags = kept(&self.flags, &map, live, |flags| flags);
        self.bytes = kept(&self.bytes, &map, live, |bytes| bytes);
        self.parents = kept(&self.parents, &map, live, |parent| {
            map.get(parent).unwrap_or(NO_PARENT)
        });
        self.counts.retain_mut(|(slot, _)| match map.get(*slot) {
            Some(new) => {
                *slot = new;
                true
            }
            None => false,
        });
        self.live = live;
        match self.children.remapped(&map, &self.parents) {
            Some(children) => self.children = children,
            None => {
                tracing::error!("child lists disagree with parents during compaction; rebuilt");
                self.relink();
            }
        }
        self.shrink_to_fit();
        map
    }

    fn kept_names(&self, map: &Remap, live: usize) -> (String, Vec<u32>) {
        let ids = || (0..self.slots()).filter(|&id| map.get(id).is_some());
        let bytes = ids().map(|id| self.name_unchecked(id).len() + 1).sum();
        let (mut names, mut offsets) = (String::with_capacity(bytes), Vec::with_capacity(live));
        for id in ids() {
            offsets.push(names.len() as u32);
            names.push_str(self.name_unchecked(id));
            names.push(NAME_END);
        }
        (names, offsets)
    }
}

#[cfg(test)]
#[path = "compact_oracle_tests.rs"]
mod oracle_tests;

#[cfg(test)]
mod tests {
    use super::NameTable;
    use crate::table::Flags;
    use crate::table::tests::{tree, win};

    #[test]
    fn remap_get_is_none_for_removed_and_out_of_range_ids() {
        let (mut t, [root, users, ann, notes, _]) = tree();
        t.remove(ann);
        let map = t.compact();
        assert_eq!(map.get(root), Some(0));
        assert_eq!(map.get(users), Some(1));
        assert_eq!(map.get(ann), None);
        assert_eq!(map.get(notes), None);
        assert_eq!(map.get(u32::MAX), None);
        assert_eq!(map.get(t.slots()), None);
        assert_eq!(size_of::<Option<std::num::NonZeroU32>>(), 4);
    }

    #[test]
    fn compact_drops_removed_rewrites_ids_and_keeps_paths() {
        let (mut t, [root, users, ann, notes, docs]) = tree();
        let extra = t.push(users, "bob", Flags::FOLDER).unwrap();
        t.rename(extra, users, "carol").unwrap();
        t.remove(ann);
        let map = t.compact();
        assert_eq!(map.get(root), Some(0));
        assert_eq!(map.get(users), Some(1));
        assert_eq!(map.get(ann), None);
        assert_eq!(map.get(notes), None);
        assert_eq!(map.get(docs), None);
        let carol = map.get(extra).unwrap();
        assert_eq!(win(&t, carol).as_deref(), Some("C:\\Users\\carol"));
        assert_eq!((t.len(), t.slots()), (3, 3));
        assert!(!t.names.contains("bob"));
    }

    #[test]
    fn removal_finds_the_subtree_after_compaction_and_after_loading_raw_parts() {
        let (mut t, [_, users, _, notes, _]) = tree();
        t.remove(notes);
        let map = t.compact();
        let (names, offsets, parents, flags) = t.raw_parts();
        let mut loaded = NameTable::from_raw_parts(
            names.to_owned(),
            (offsets.to_vec(), parents.to_vec(), flags.to_vec()),
            (t.stored_totals().0.to_vec(), t.stored_totals().1.collect()),
        )
        .unwrap();
        let users = map.get(users).unwrap();
        assert_eq!(t.remove(users), 3);
        assert_eq!(loaded.remove(users), 3);
        assert_eq!((t.len(), loaded.len()), (1, 1));
    }
}
