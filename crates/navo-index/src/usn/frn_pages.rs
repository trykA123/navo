use std::collections::TryReserveError;

/// Records per page.
pub(super) const PAGE: usize = 1024;
/// Marks a free slot, so it is never stored as an id.
pub(super) const EMPTY: u32 = u32::MAX;
const PAGE_BYTES: usize = PAGE * (size_of::<u32>() + size_of::<u16>());
const RECORD_LIMIT: u64 = 1 << 32;
// A new page is allocated only while the pages stay within 24 B per record plus this slack.
const BYTES_PER_RECORD: usize = 24;
const BUDGET_SLACK: usize = 256 * 1024;

#[derive(Debug, Clone)]
pub(super) struct Page {
    ids: Box<[u32]>,
    seqs: Box<[u16]>,
    live: u16,
}

impl Page {
    fn try_new() -> Result<Self, TryReserveError> {
        let mut ids = Vec::new();
        ids.try_reserve_exact(PAGE)?;
        ids.resize(PAGE, EMPTY);
        let mut seqs = Vec::new();
        seqs.try_reserve_exact(PAGE)?;
        seqs.resize(PAGE, 0);
        Ok(Self {
            ids: ids.into_boxed_slice(),
            seqs: seqs.into_boxed_slice(),
            live: 0,
        })
    }

    fn get(&self, slot: usize, seq: u16) -> Option<&u32> {
        let id = &self.ids[slot];
        (*id != EMPTY && self.seqs[slot] == seq).then_some(id)
    }

    fn place(&mut self, slot: usize, seq: u16, id: u32) -> Placed {
        let old = self.ids[slot];
        if old == EMPTY {
            self.ids[slot] = id;
            self.seqs[slot] = seq;
            self.live += 1;
            Placed::Inserted
        } else if self.seqs[slot] == seq {
            self.ids[slot] = id;
            Placed::Updated(old)
        } else {
            Placed::Refused
        }
    }

    fn take(&mut self, slot: usize) -> u32 {
        self.live -= 1;
        std::mem::replace(&mut self.ids[slot], EMPTY)
    }

    /// Keeps, rewrites (`Some`) or frees (`None`) every held slot; returns how many were freed.
    fn rewrite(&mut self, base: u64, step: &mut impl FnMut(u64, u16, u32) -> Option<u32>) -> usize {
        let mut freed = 0;
        for slot in 0..PAGE {
            let id = self.ids[slot];
            if id == EMPTY {
                continue;
            }
            match step(base + slot as u64, self.seqs[slot], id) {
                Some(new) => self.ids[slot] = new,
                None => {
                    self.take(slot);
                    freed += 1;
                }
            }
        }
        freed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Placed {
    Inserted,
    Updated(u32),
    /// Nothing changed: an `EMPTY` id, a record at or past 2^32, another `seq` in the slot, or no budget for a page.
    Refused,
}

/// Narrow file references by MFT record: a directory of 1024-record pages, each slot an id and its `seq`.
#[derive(Debug, Clone, Default)]
pub(super) struct DensePages {
    directory: Vec<Option<Page>>,
    len: usize,
    pages: usize,
}

fn split(record: u64) -> Option<(usize, usize)> {
    let page = PAGE as u64;
    (record < RECORD_LIMIT).then(|| ((record / page) as usize, (record % page) as usize))
}

impl DensePages {
    pub(super) fn get(&self, record: u64, seq: u16) -> Option<u32> {
        self.get_ref(record, seq).copied()
    }

    pub(super) fn get_ref(&self, record: u64, seq: u16) -> Option<&u32> {
        let (index, slot) = split(record)?;
        self.directory.get(index)?.as_ref()?.get(slot, seq)
    }

    /// Places `id` or refuses; an `Err` comes only from an allocation and leaves the pages unchanged.
    pub(super) fn try_insert(
        &mut self,
        record: u64,
        seq: u16,
        id: u32,
    ) -> Result<Placed, TryReserveError> {
        let Some((index, slot)) = split(record).filter(|_| id != EMPTY) else {
            return Ok(Placed::Refused);
        };
        if let Some(Some(page)) = self.directory.get_mut(index) {
            let placed = page.place(slot, seq, id);
            if placed == Placed::Inserted {
                self.len += 1;
            }
            return Ok(placed);
        }
        if !self.fits_new_page(index) {
            return Ok(Placed::Refused);
        }
        let mut page = Page::try_new()?;
        if index >= self.directory.len() {
            let target = self.directory_slots_after(index);
            // Last fallible step: on `Err` the staged page drops and nothing else has changed.
            self.directory
                .try_reserve_exact(target - self.directory.len())?;
            self.directory.resize_with(index + 1, || None);
        }
        page.place(slot, seq, id);
        self.directory[index] = Some(page);
        self.pages += 1;
        self.len += 1;
        Ok(Placed::Inserted)
    }

    /// Directory capacity if `index` were added now: the gap plus an eighth of the current length.
    fn directory_slots_after(&self, index: usize) -> usize {
        let len = self.directory.len();
        if index < len {
            return self.directory.capacity();
        }
        let growth = (index + 1 - len).max(len / 8);
        self.directory.capacity().max(len + growth)
    }

    fn fits_new_page(&self, index: usize) -> bool {
        let slots = self.directory_slots_after(index);
        let after = slots * size_of::<Option<Page>>() + (self.pages + 1) * PAGE_BYTES;
        after <= BYTES_PER_RECORD * (self.len + 1) + BUDGET_SLACK
    }

    /// Removes the key and frees its page when that was the page's last record.
    pub(super) fn remove(&mut self, record: u64, seq: u16) -> Option<u32> {
        let (index, slot) = split(record)?;
        let entry = self.directory.get_mut(index)?;
        let page = entry.as_mut()?;
        page.get(slot, seq)?;
        let id = page.take(slot);
        if page.live == 0 {
            *entry = None;
            self.pages -= 1;
        }
        self.len -= 1;
        Some(id)
    }

    pub(super) fn retain(&mut self, mut keep: impl FnMut(u64, u16, u32) -> bool) {
        self.rewrite(|record, seq, id| keep(record, seq, id).then_some(id));
    }

    /// Rewrites ids in place and drops those `new_id` rejects. Pairs whose new id is `EMPTY`
    /// leave the pages and come back as `(record, seq, EMPTY)`.
    pub(super) fn remap(
        &mut self,
        mut new_id: impl FnMut(u32) -> Option<u32>,
    ) -> Vec<(u64, u16, u32)> {
        let mut moved = Vec::new();
        self.rewrite(|record, seq, id| match new_id(id) {
            Some(EMPTY) => {
                moved.push((record, seq, EMPTY));
                None
            }
            other => other,
        });
        moved
    }

    fn rewrite(&mut self, mut step: impl FnMut(u64, u16, u32) -> Option<u32>) {
        for (index, entry) in self.directory.iter_mut().enumerate() {
            let Some(page) = entry else { continue };
            self.len -= page.rewrite((index * PAGE) as u64, &mut step);
            if page.live == 0 {
                *entry = None;
                self.pages -= 1;
            }
        }
    }

    /// Held `(record, seq, id)` triples in record order.
    pub(super) fn iter(&self) -> impl Iterator<Item = (u64, u16, u32)> + '_ {
        let pages = self.directory.iter().enumerate();
        pages
            .filter_map(|(index, entry)| Some(((index * PAGE) as u64, entry.as_ref()?)))
            .flat_map(|(base, page)| {
                let slots = page.ids.iter().zip(page.seqs.iter()).enumerate();
                slots
                    .filter(|&(_, (&id, _))| id != EMPTY)
                    .map(move |(slot, (&id, &seq))| (base + slot as u64, seq, id))
            })
    }

    /// Held ids in record order.
    pub(super) fn values(&self) -> impl Iterator<Item = &u32> + '_ {
        let pages = self.directory.iter().flatten();
        pages.flat_map(|page| page.ids.iter().filter(|&&id| id != EMPTY))
    }

    pub(super) fn len(&self) -> usize {
        self.len
    }

    /// Frees every page and keeps the directory's capacity.
    pub(super) fn clear(&mut self) {
        self.directory.clear();
        self.len = 0;
        self.pages = 0;
    }

    pub(super) fn shrink_to_fit(&mut self) {
        while matches!(self.directory.last(), Some(None)) {
            self.directory.pop();
        }
        self.directory.shrink_to_fit();
    }

    /// Heap bytes of the directory and of the pages.
    pub(super) fn memory_parts(&self) -> (usize, usize) {
        (
            self.directory.capacity() * size_of::<Option<Page>>(),
            self.pages * PAGE_BYTES,
        )
    }
}

#[cfg(test)]
#[path = "frn_pages_tests.rs"]
mod tests;
