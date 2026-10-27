mod pattern;

use crate::Listing;
pub use pattern::{Pattern, PatternError};

const WORD: u64 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpanMode {
    Mark,
    Unmark,
    Toggle,
}

impl SpanMode {
    #[must_use]
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Mark => "mark",
            Self::Unmark => "unmark",
            Self::Toggle => "toggle",
        }
    }

    #[must_use]
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        match keyword {
            "mark" => Some(Self::Mark),
            "unmark" => Some(Self::Unmark),
            "toggle" => Some(Self::Toggle),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Span {
    anchor: u64,
    base: Vec<u64>,
    mode: SpanMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Selection {
    len: u64,
    bits: Vec<u64>,
    anchor: Option<u64>,
    span: Option<Span>,
}

impl Selection {
    #[must_use]
    pub fn new(len: u64) -> Self {
        let words = usize::try_from(len.div_ceil(WORD)).unwrap_or(usize::MAX);
        Self {
            len,
            bits: vec![0; words],
            anchor: None,
            span: None,
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

    pub fn extend(&mut self, from_cursor: u64, to: u64, mode: SpanMode) {
        let Some(last) = self.len.checked_sub(1) else {
            return;
        };
        let span = self.span.get_or_insert_with(|| Span {
            anchor: from_cursor.min(last),
            base: self.bits.clone(),
            mode,
        });
        self.bits.clone_from(&span.base);
        let (anchor, mode) = (span.anchor, span.mode);
        if to > last {
            return;
        }
        for row in anchor.min(to)..=anchor.max(to) {
            let on = match mode {
                SpanMode::Mark => true,
                SpanMode::Unmark => false,
                SpanMode::Toggle => !self.is_marked(row),
            };
            self.set(row, on);
        }
    }

    pub fn end_span(&mut self) {
        self.span = None;
    }

    pub fn mark_extension(&mut self, listing: &Listing, ext: Option<&str>, on: bool) {
        self.each_row(listing, false, |name, _| {
            let matches = match (extension(name), ext) {
                (Some(actual), Some(wanted)) => actual.eq_ignore_ascii_case(wanted),
                (None, None) => true,
                _ => false,
            };
            matches.then_some(on)
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
        self.end_span();
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

pub(crate) fn extension(name: &str) -> Option<&str> {
    name.rfind('.')
        .filter(|dot| *dot > 0 && dot + 1 < name.len())
        .map(|dot| &name[dot + 1..])
}

#[cfg(test)]
mod tests;
