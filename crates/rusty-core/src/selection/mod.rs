mod pattern;

use crate::Listing;
pub use pattern::{Pattern, PatternError};

const WORD: u64 = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    len: u64,
    bits: Vec<u64>,
    anchor: Option<u64>,
}

impl Selection {
    #[must_use]
    pub fn new(len: u64) -> Self {
        let words = usize::try_from(len.div_ceil(WORD)).unwrap_or(usize::MAX);
        Self {
            len,
            bits: vec![0; words],
            anchor: None,
        }
    }

    #[must_use]
    pub fn len(&self) -> u64 {
        self.len
    }

    #[must_use]
    pub fn count(&self) -> u64 {
        self.bits.iter().map(|w| u64::from(w.count_ones())).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|w| *w == 0)
    }

    #[must_use]
    pub fn is_marked(&self, row: u64) -> bool {
        self.word(row)
            .is_some_and(|(i, mask)| self.bits[i] & mask != 0)
    }

    #[must_use]
    pub fn marked(&self) -> Vec<u64> {
        let mut rows = Vec::new();
        for (base, word) in (0..).step_by(64).zip(&self.bits) {
            let mut rest = *word;
            while rest != 0 {
                rows.push(base + u64::from(rest.trailing_zeros()));
                rest &= rest - 1;
            }
        }
        rows
    }

    #[must_use]
    pub fn anchor(&self) -> Option<u64> {
        self.anchor
    }

    pub fn toggle(&mut self, row: u64) -> bool {
        let Some((i, mask)) = self.word(row) else {
            return false;
        };
        self.bits[i] ^= mask;
        self.anchor = Some(row);
        self.bits[i] & mask != 0
    }

    pub fn mark_range(&mut self, to: u64) {
        let Some(last) = self.len.checked_sub(1) else {
            return;
        };
        let (from, to) = match self.anchor {
            Some(anchor) => (anchor.min(to), anchor.max(to).min(last)),
            None if to <= last => (to, to),
            None => return,
        };
        for row in from..=to {
            self.set(row, true);
        }
    }

    pub fn mark_pattern(&mut self, listing: &Listing, pattern: &Pattern, folders: bool) {
        self.each_row(listing, folders, |entry_name, _| {
            pattern.matches(entry_name).then_some(true)
        });
    }

    pub fn unmark_pattern(&mut self, listing: &Listing, pattern: &Pattern, folders: bool) {
        self.each_row(listing, folders, |entry_name, _| {
            pattern.matches(entry_name).then_some(false)
        });
    }

    pub fn mark_all(&mut self, listing: &Listing, folders: bool) {
        self.each_row(listing, folders, |_, _| Some(true));
    }

    pub fn invert(&mut self, listing: &Listing, folders: bool) {
        self.each_row(listing, folders, |_, marked| Some(!marked));
    }

    pub fn mark_rows(&mut self, rows: impl IntoIterator<Item = u64>) {
        for row in rows {
            self.set(row, true);
        }
    }

    pub fn clear(&mut self) {
        self.bits.fill(0);
        self.anchor = None;
    }

    fn each_row(
        &mut self,
        listing: &Listing,
        folders: bool,
        mut decide: impl FnMut(&str, bool) -> Option<bool>,
    ) {
        let rows = listing.entries().iter().zip(0..self.len);
        for (entry, row) in rows.filter(|(e, _)| folders || !e.is_folder) {
            if let Some(on) = decide(&entry.name, self.is_marked(row)) {
                self.set(row, on);
            }
        }
    }

    fn set(&mut self, row: u64, on: bool) {
        if let Some((i, mask)) = self.word(row) {
            if on {
                self.bits[i] |= mask;
            } else {
                self.bits[i] &= !mask;
            }
        }
    }

    fn word(&self, row: u64) -> Option<(usize, u64)> {
        if row >= self.len {
            return None;
        }
        let i = usize::try_from(row / WORD).ok()?;
        (i < self.bits.len()).then_some((i, 1 << (row % WORD)))
    }
}

#[cfg(test)]
mod tests;
