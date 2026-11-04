use std::collections::TryReserveError;

use super::NO_PARENT;
use super::compact::Remap;
use super::growth::{grow_column, trim_column};

const NO_LINK: u32 = u32::MAX;

#[derive(Debug, Clone, Default)]
pub(super) struct Children {
    first: Vec<u32>,
    next: Vec<u32>,
    previous: Vec<u32>,
    waiting: Vec<u32>,
}

impl Children {
    pub(super) fn with_capacity(entries: usize) -> Self {
        Self {
            first: Vec::with_capacity(entries),
            next: Vec::with_capacity(entries),
            previous: Vec::with_capacity(entries),
            waiting: Vec::new(),
        }
    }

    pub(super) fn try_reserve_total(&mut self, entries: usize) -> Result<(), TryReserveError> {
        let additional = entries.saturating_sub(self.first.len());
        self.first.try_reserve_exact(additional)?;
        self.next.try_reserve_exact(additional)?;
        self.previous.try_reserve_exact(additional)?;
        Ok(())
    }

    pub(super) fn reserve_for_push(&mut self) {
        grow_column(&mut self.first);
        grow_column(&mut self.next);
        grow_column(&mut self.previous);
    }

    pub(super) fn trim_slack(&mut self) {
        trim_column(&mut self.first);
        trim_column(&mut self.next);
        trim_column(&mut self.previous);
    }

    pub(super) fn rebuild(parents: &[u32], is_live: impl Fn(u32) -> bool) -> Self {
        let mut children = Self::with_capacity(parents.len());
        children.first.resize(parents.len(), NO_LINK);
        children.next.resize(parents.len(), NO_LINK);
        children.previous.resize(parents.len(), NO_LINK);
        for (id, &parent) in (0u32..).zip(parents) {
            if is_live(id) {
                children.attach(id, parent);
            }
        }
        children
    }

    /// Renumbers through a `NameTable::compact` map, keeping each parent's child order.
    /// Returns `None` when the lists disagree with `parents`, so the caller rebuilds instead.
    pub(super) fn remapped(&self, map: &Remap, parents: &[u32]) -> Option<Self> {
        let mut fresh = Self::with_capacity(parents.len());
        for links in [&mut fresh.first, &mut fresh.next, &mut fresh.previous] {
            links.resize(parents.len(), NO_LINK);
        }
        // Each slot sits in at most one list, so more links than slots means a cycle.
        let (mut budget, mut linked) = (self.first.len(), 0);
        for old in 0..self.first.len() as u32 {
            let Some(new) = map.get(old) else {
                continue;
            };
            let children = self
                .bounded_list(old, &mut budget)
                .map(|child| child.map(|child| map.get(child)));
            linked += fresh.append_list(new, children, parents)?;
        }
        (linked
            == parents
                .iter()
                .filter(|&&parent| parent != NO_PARENT)
                .count())
        .then_some(fresh)
    }

    /// Children of `parent`; yields `None` once a link leaves the table or `budget` runs out.
    fn bounded_list<'a>(
        &'a self,
        parent: u32,
        budget: &'a mut usize,
    ) -> impl Iterator<Item = Option<u32>> + 'a {
        let mut child = self.first[parent as usize];
        std::iter::from_fn(move || {
            if child == NO_LINK {
                return None;
            }
            let at = std::mem::replace(&mut child, NO_LINK);
            if *budget == 0 || at as usize >= self.first.len() {
                return Some(None);
            }
            *budget -= 1;
            child = self.next[at as usize];
            Some(Some(at))
        })
    }

    /// Appends renumbered children in order; `None` items are corrupt links, `Some(None)` removed.
    fn append_list(
        &mut self,
        parent: u32,
        children: impl Iterator<Item = Option<Option<u32>>>,
        parents: &[u32],
    ) -> Option<usize> {
        let (mut tail, mut linked) = (NO_LINK, 0);
        for child in children {
            let Some(child) = child? else {
                continue;
            };
            if parents.get(child as usize) != Some(&parent) {
                return None;
            }
            match tail {
                NO_LINK => self.first[parent as usize] = child,
                _ => self.next[tail as usize] = child,
            }
            self.previous[child as usize] = tail;
            (tail, linked) = (child, linked + 1);
        }
        Some(linked)
    }

    pub(super) fn memory_bytes(&self) -> usize {
        (self.first.capacity()
            + self.next.capacity()
            + self.previous.capacity()
            + self.waiting.capacity())
            * size_of::<u32>()
    }

    pub(super) fn shrink_to_fit(&mut self) {
        self.first.shrink_to_fit();
        self.next.shrink_to_fit();
        self.previous.shrink_to_fit();
        self.waiting.shrink_to_fit();
    }

    pub(super) fn add(&mut self, id: u32, parents: &[u32]) {
        self.first.push(NO_LINK);
        self.next.push(NO_LINK);
        self.previous.push(NO_LINK);
        self.attach(id, parents[id as usize]);
        if self
            .waiting
            .iter()
            .any(|&child| parents[child as usize] == id)
        {
            let ready: Vec<u32> = self
                .waiting
                .extract_if(.., |&mut child| parents[child as usize] == id)
                .collect();
            for child in ready {
                self.attach(child, id);
            }
        }
    }

    pub(super) fn reparent(&mut self, child: u32, from: u32, to: u32) {
        self.detach(child, from);
        self.attach(child, to);
    }

    pub(super) fn detach(&mut self, child: u32, parent: u32) {
        if parent as usize >= self.first.len() {
            self.waiting.retain(|&waiting| waiting != child);
            return;
        }
        let at = child as usize;
        let (previous, next) = (self.previous[at], self.next[at]);
        if previous != NO_LINK {
            self.next[previous as usize] = next;
        } else if self.first[parent as usize] == child {
            self.first[parent as usize] = next;
        }
        if next != NO_LINK {
            self.previous[next as usize] = previous;
        }
        self.next[at] = NO_LINK;
        self.previous[at] = NO_LINK;
    }

    pub(super) fn of(&self, parent: u32) -> impl Iterator<Item = u32> + '_ {
        let first = self.first.get(parent as usize).copied().unwrap_or(NO_LINK);
        let linked = |child: u32| (child != NO_LINK).then_some(child);
        std::iter::successors(linked(first), move |&child| {
            linked(self.next[child as usize])
        })
    }

    pub(super) fn first_child(&self, parent: u32) -> Option<u32> {
        self.first
            .get(parent as usize)
            .copied()
            .filter(|&child| child != NO_LINK)
    }

    pub(super) fn next_sibling(&self, child: u32) -> Option<u32> {
        self.next
            .get(child as usize)
            .copied()
            .filter(|&next| next != NO_LINK)
    }

    fn attach(&mut self, child: u32, parent: u32) {
        if parent as usize >= self.first.len() {
            if parent != NO_PARENT {
                self.waiting.push(child);
            }
            return;
        }
        let head = self.first[parent as usize];
        self.next[child as usize] = head;
        self.previous[child as usize] = NO_LINK;
        if head != NO_LINK {
            self.previous[head as usize] = child;
        }
        self.first[parent as usize] = child;
    }
}

#[cfg(test)]
impl Children {
    pub(super) fn capacities(&self) -> [usize; 3] {
        [
            self.first.capacity(),
            self.next.capacity(),
            self.previous.capacity(),
        ]
    }

    pub(super) fn set_next(&mut self, id: u32, next: u32) {
        self.next[id as usize] = next;
    }
}

#[cfg(test)]
impl super::NameTable {
    pub(crate) fn child_names(&self, parent: u32) -> Vec<&str> {
        self.children
            .of(parent)
            .filter_map(|child| self.name(child))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::compact::Remap;
    use super::super::tests::tree;
    use super::super::{Flags, NameTable};
    use super::{Children, NO_LINK};

    /// Remap and new parents from a compacted copy, before the original is corrupted.
    fn compacted(t: &NameTable) -> (Remap, Vec<u32>) {
        let mut copy = t.clone();
        let map = copy.compact();
        (map, copy.parents)
    }

    fn remapped(t: &NameTable, map: &Remap, parents: &[u32]) -> Option<Children> {
        t.children.remapped(map, parents)
    }

    #[test]
    fn an_out_of_range_next_link_returns_none() {
        let (mut t, [_, _, _, notes, _]) = tree();
        let (map, parents) = compacted(&t);
        t.children.next[notes as usize] = t.slots() + 5;
        assert!(remapped(&t, &map, &parents).is_none());
    }

    #[test]
    fn a_self_looping_list_returns_none() {
        let (mut t, [_, _, _, notes, _]) = tree();
        let (map, parents) = compacted(&t);
        t.children.next[notes as usize] = notes;
        assert!(remapped(&t, &map, &parents).is_none());
    }

    #[test]
    fn a_two_entry_cycle_returns_none() {
        let (mut t, [_, _, ann, notes, docs]) = tree();
        let (map, parents) = compacted(&t);
        assert_eq!(t.children.of(ann).collect::<Vec<_>>(), [docs, notes]);
        t.children.next[notes as usize] = docs;
        assert!(remapped(&t, &map, &parents).is_none());
    }

    #[test]
    fn a_removed_child_in_a_live_list_is_skipped_and_the_others_keep_their_order() {
        let (mut t, [_, _, ann, notes, docs]) = tree();
        let extra = t.push(ann, "extra.txt", Flags::NONE).unwrap();
        t.rename(notes, ann, "notes.md").unwrap();
        assert_eq!(t.children.of(ann).collect::<Vec<_>>(), [notes, extra, docs]);
        t.mark_deleted(extra);
        let (map, parents) = compacted(&t);
        let fresh = remapped(&t, &map, &parents).unwrap();
        let (notes, docs) = (map.get(notes).unwrap(), map.get(docs).unwrap());
        assert_eq!(
            fresh.of(map.get(ann).unwrap()).collect::<Vec<_>>(),
            [notes, docs]
        );
    }

    #[test]
    fn a_waiting_child_stays_unlinked_and_the_count_still_matches() {
        let (mut t, [root, users, ..]) = tree();
        let waiting = t.push(t.slots() + 10, "orphan", Flags::NONE).unwrap();
        assert_eq!(t.children.waiting, [waiting]);
        let (map, parents) = compacted(&t);
        let fresh = remapped(&t, &map, &parents).unwrap();
        let orphan = map.get(waiting).unwrap();
        assert_eq!(parents[orphan as usize], super::NO_PARENT);
        assert!((0..parents.len() as u32).all(|p| fresh.of(p).all(|c| c != orphan)));
        assert_eq!(fresh.previous[orphan as usize], NO_LINK);
        assert_eq!(
            fresh.of(map.get(root).unwrap()).collect::<Vec<_>>(),
            [map.get(users).unwrap()]
        );
    }
}
