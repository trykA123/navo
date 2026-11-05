use std::collections::TryReserveError;

use super::NO_PARENT;
use super::compact::Remap;
use super::growth::{grow_column, trim_column};

const NO_LINK: u32 = u32::MAX;

/// Child lists as links per slot; head rows exist only for folders and for ids that got a child.
#[derive(Debug, Clone, Default)]
pub(super) struct Children {
    /// (parent id, first child), sorted by parent id.
    heads: Vec<(u32, u32)>,
    next: Vec<u32>,
    previous: Vec<u32>,
    waiting: Vec<u32>,
}

impl Children {
    #[cfg(test)]
    pub(super) fn column_bytes(&self) -> [(&'static str, usize); 4] {
        [
            (
                "children.heads",
                self.heads.capacity() * size_of::<(u32, u32)>(),
            ),
            ("children.next", self.next.capacity() * size_of::<u32>()),
            (
                "children.previous",
                self.previous.capacity() * size_of::<u32>(),
            ),
            (
                "children.waiting",
                self.waiting.capacity() * size_of::<u32>(),
            ),
        ]
    }

    pub(super) fn with_capacity(entries: usize) -> Self {
        Self {
            heads: Vec::new(),
            next: Vec::with_capacity(entries),
            previous: Vec::with_capacity(entries),
            waiting: Vec::new(),
        }
    }

    pub(super) fn try_reserve_total(
        &mut self,
        entries: usize,
        folders: usize,
    ) -> Result<(), TryReserveError> {
        let additional = entries.saturating_sub(self.next.len());
        self.heads
            .try_reserve_exact(folders.saturating_sub(self.heads.len()))?;
        self.next.try_reserve_exact(additional)?;
        self.previous.try_reserve_exact(additional)?;
        Ok(())
    }

    pub(super) fn reserve_for_push(&mut self) {
        grow_column(&mut self.next);
        grow_column(&mut self.previous);
    }

    pub(super) fn trim_slack(&mut self, slack: usize) {
        trim_column(&mut self.heads, slack);
        trim_column(&mut self.next, slack);
        trim_column(&mut self.previous, slack);
    }

    /// Links every live entry under its parent, with a head row for each live folder and each
    /// parent of a live child. Scratch is one bit per slot.
    pub(super) fn rebuild(
        parents: &[u32],
        is_live: impl Fn(u32) -> bool,
        is_folder: impl Fn(u32) -> bool,
    ) -> Self {
        let slots = parents.len();
        let mut has_row = vec![0u64; slots.div_ceil(64)];
        let mut mark = |id: u32| has_row[id as usize / 64] |= 1 << (id % 64);
        for (id, &parent) in (0u32..).zip(parents).filter(|&(id, _)| is_live(id)) {
            if is_folder(id) {
                mark(id);
            }
            if (parent as usize) < slots {
                mark(parent);
            }
        }
        let rows = has_row.iter().map(|word| word.count_ones() as usize).sum();
        let mut children = Self {
            heads: Vec::with_capacity(rows),
            next: vec![NO_LINK; slots],
            previous: vec![NO_LINK; slots],
            waiting: Vec::new(),
        };
        let marked = |id: u32| has_row[id as usize / 64] & (1 << (id % 64)) != 0;
        children.heads.extend(
            (0u32..)
                .take(slots)
                .filter(|&id| marked(id))
                .map(|id| (id, NO_LINK)),
        );
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
        fresh.next.resize(parents.len(), NO_LINK);
        fresh.previous.resize(parents.len(), NO_LINK);
        fresh.heads.reserve_exact(self.heads.len());
        // Each slot sits in at most one list, so more links than slots means a cycle.
        let (mut budget, mut linked) = (self.slots(), 0);
        for &(old, first) in &self.heads {
            let Some(new) = map.get(old) else {
                continue;
            };
            fresh.heads.push((new, NO_LINK));
            let children = self
                .bounded_list(first, &mut budget)
                .map(|child| child.map(|child| map.get(child)));
            linked += fresh.append_list(children, parents)?;
        }
        (linked
            == parents
                .iter()
                .filter(|&&parent| parent != NO_PARENT)
                .count())
        .then_some(fresh)
    }

    /// The list starting at `first`; yields `None` once a link leaves the table or `budget` runs out.
    fn bounded_list<'a>(
        &'a self,
        first: u32,
        budget: &'a mut usize,
    ) -> impl Iterator<Item = Option<u32>> + 'a {
        let mut child = first;
        std::iter::from_fn(move || {
            if child == NO_LINK {
                return None;
            }
            let at = std::mem::replace(&mut child, NO_LINK);
            if *budget == 0 || at as usize >= self.slots() {
                return Some(None);
            }
            *budget -= 1;
            child = self.next[at as usize];
            Some(Some(at))
        })
    }

    /// Appends renumbered children in order under the last head row; `None` items are corrupt
    /// links, `Some(None)` removed.
    fn append_list(
        &mut self,
        children: impl Iterator<Item = Option<Option<u32>>>,
        parents: &[u32],
    ) -> Option<usize> {
        let (parent, _) = *self.heads.last()?;
        let (mut tail, mut linked) = (NO_LINK, 0);
        for child in children {
            let Some(child) = child? else {
                continue;
            };
            if parents.get(child as usize) != Some(&parent) {
                return None;
            }
            match tail {
                NO_LINK => self.heads.last_mut()?.1 = child,
                _ => self.next[tail as usize] = child,
            }
            self.previous[child as usize] = tail;
            (tail, linked) = (child, linked + 1);
        }
        Some(linked)
    }

    pub(super) fn memory_bytes(&self) -> usize {
        self.heads.capacity() * size_of::<(u32, u32)>()
            + (self.next.capacity() + self.previous.capacity() + self.waiting.capacity())
                * size_of::<u32>()
    }

    pub(super) fn shrink_to_fit(&mut self) {
        self.heads.shrink_to_fit();
        self.next.shrink_to_fit();
        self.previous.shrink_to_fit();
        self.waiting.shrink_to_fit();
    }

    /// Adds slot `id`; a folder gets its head row now, any other id when it gets a child.
    pub(super) fn add(&mut self, id: u32, parents: &[u32], folder: bool) {
        self.next.push(NO_LINK);
        self.previous.push(NO_LINK);
        if folder {
            grow_column(&mut self.heads);
            self.heads.push((id, NO_LINK));
        }
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
        if parent as usize >= self.slots() {
            self.waiting.retain(|&waiting| waiting != child);
            return;
        }
        let at = child as usize;
        let (previous, next) = (self.previous[at], self.next[at]);
        if previous != NO_LINK {
            self.next[previous as usize] = next;
        } else if let Ok(row) = self.row(parent)
            && self.heads[row].1 == child
        {
            self.heads[row].1 = next;
        }
        if next != NO_LINK {
            self.previous[next as usize] = previous;
        }
        self.next[at] = NO_LINK;
        self.previous[at] = NO_LINK;
    }

    pub(super) fn of(&self, parent: u32) -> impl Iterator<Item = u32> + '_ {
        let first = self.first_child(parent).unwrap_or(NO_LINK);
        let linked = |child: u32| (child != NO_LINK).then_some(child);
        std::iter::successors(linked(first), move |&child| {
            linked(self.next[child as usize])
        })
    }

    pub(super) fn first_child(&self, parent: u32) -> Option<u32> {
        let row = self.row(parent).ok()?;
        Some(self.heads[row].1).filter(|&child| child != NO_LINK)
    }

    pub(super) fn next_sibling(&self, child: u32) -> Option<u32> {
        self.next
            .get(child as usize)
            .copied()
            .filter(|&next| next != NO_LINK)
    }

    fn slots(&self) -> usize {
        self.next.len()
    }

    fn row(&self, parent: u32) -> Result<usize, usize> {
        self.heads.binary_search_by_key(&parent, |&(id, _)| id)
    }

    fn attach(&mut self, child: u32, parent: u32) {
        if parent as usize >= self.slots() {
            if parent != NO_PARENT {
                self.waiting.push(child);
            }
            return;
        }
        let row = self.row(parent).unwrap_or_else(|at| {
            grow_column(&mut self.heads);
            self.heads.insert(at, (parent, NO_LINK));
            at
        });
        let head = std::mem::replace(&mut self.heads[row].1, child);
        self.next[child as usize] = head;
        self.previous[child as usize] = NO_LINK;
        if head != NO_LINK {
            self.previous[head as usize] = child;
        }
    }
}

#[cfg(test)]
impl Children {
    pub(super) fn capacities(&self) -> [usize; 3] {
        [
            self.heads.capacity(),
            self.next.capacity(),
            self.previous.capacity(),
        ]
    }

    pub(super) fn spares(&self) -> [(usize, usize); 3] {
        [
            (self.heads.capacity(), self.heads.len()),
            (self.next.capacity(), self.next.len()),
            (self.previous.capacity(), self.previous.len()),
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
#[path = "children_flag_tests.rs"]
mod flag_tests;

#[cfg(test)]
#[path = "children_tests.rs"]
mod tests;
