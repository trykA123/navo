use std::collections::BTreeSet;

use super::search::Hit;
use crate::format;

pub(super) const HIT_CAP: usize = 100_000;

pub(super) type HitKey = (u32, u32);

#[derive(Default)]
pub(super) struct Hits {
    list: Vec<Hit>,
    pages: BTreeSet<u32>,
    current: Option<HitKey>,
    is_capped: bool,
    revision: u64,
}

impl Hits {
    pub(super) fn clear(&mut self) {
        let revision = self.revision + 1;
        *self = Self {
            revision,
            ..Self::default()
        };
    }

    fn span(&self, page: u32) -> std::ops::Range<usize> {
        let start = self.list.partition_point(|hit| hit.page < page);
        let end = self.list.partition_point(|hit| hit.page <= page);
        start..end
    }

    pub(super) fn add_page(&mut self, page: u32, mut hits: Vec<Hit>) {
        let span = self.span(page);
        let old = span.len();
        let room = HIT_CAP.saturating_sub(self.list.len() - old);
        if hits.len() > room {
            hits.truncate(room);
            self.is_capped = true;
        }
        if old == 0 && hits.is_empty() {
            return;
        }
        if hits.is_empty() {
            self.pages.remove(&page);
        } else {
            self.pages.insert(page);
        }
        self.list.splice(span, hits);
        self.revision += 1;
    }

    pub(super) fn total(&self) -> usize {
        self.list.len()
    }

    pub(super) fn is_capped(&self) -> bool {
        self.is_capped
    }

    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub(super) fn page_list(&self) -> impl Iterator<Item = u32> + '_ {
        self.pages.iter().copied()
    }

    pub(super) fn on(&self, page: u32) -> &[Hit] {
        &self.list[self.span(page)]
    }

    pub(super) fn all(&self) -> &[Hit] {
        &self.list
    }

    pub(super) fn index_of(&self, key: HitKey) -> Option<usize> {
        self.list.binary_search_by_key(&key, Hit::key).ok()
    }

    pub(super) fn get(&self, row: usize) -> Option<Hit> {
        self.list.get(row).copied()
    }

    pub(super) fn current(&self) -> Option<Hit> {
        self.current.and_then(|key| self.get(self.index_of(key)?))
    }

    pub(super) fn set_current(&mut self, hit: Option<Hit>) {
        self.current = hit.map(|hit| hit.key());
    }

    pub(super) fn ordinal(&self) -> Option<usize> {
        self.index_of(self.current?).map(|at| at + 1)
    }

    pub(super) fn step(&self, from: HitKey, backward: bool) -> Option<Hit> {
        let after = self.list.partition_point(|hit| hit.key() <= from);
        if backward {
            let before = self.list.partition_point(|hit| hit.key() < from);
            before.checked_sub(1).and_then(|at| self.get(at))
        } else {
            self.get(after)
        }
    }

    pub(super) fn first_from(&self, page: u32) -> Option<Hit> {
        self.get(self.list.partition_point(|hit| hit.page < page))
            .or_else(|| self.wrap(false))
    }

    pub(super) fn wrap(&self, backward: bool) -> Option<Hit> {
        if backward {
            self.list.last().copied()
        } else {
            self.list.first().copied()
        }
    }
}

pub(super) struct Progress {
    pub is_complete: bool,
    pub is_partial: bool,
}

pub(super) fn count_label(hits: &Hits, progress: &Progress) -> String {
    let count = if hits.is_capped() {
        format!("{}+", format::bytes(hits.total() as u64))
    } else {
        format::bytes(hits.total() as u64)
    };
    let tail = match (progress.is_complete, progress.is_partial) {
        (false, _) => " so far",
        (true, true) => " · some pages skipped",
        (true, false) => "",
    };
    match (hits.total(), hits.ordinal()) {
        (0, _) if progress.is_complete => format!("No hits{tail}"),
        (0, _) => "No hits so far".to_owned(),
        (_, Some(place)) => format!("{} of {count}{tail}", format::bytes(place as u64)),
        (1, None) => format!("1 hit{tail}"),
        (_, None) => format!("{count} hits{tail}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(page: u32, start: u32) -> Hit {
        Hit {
            page,
            start,
            len: 2,
            y: 0.,
        }
    }

    const DONE: Progress = Progress {
        is_complete: true,
        is_partial: false,
    };
    const RUNNING: Progress = Progress {
        is_complete: false,
        is_partial: false,
    };

    #[test]
    fn stepping_moves_through_pages_in_order_and_counts_its_place() {
        let mut hits = Hits::default();
        hits.add_page(4, vec![hit(4, 1)]);
        hits.add_page(1, vec![hit(1, 3), hit(1, 9)]);
        let next = hits.step((1, 3), false);
        assert_eq!(next.map(|hit| hit.key()), Some((1, 9)));
        hits.set_current(hits.step((1, 9), false));
        assert_eq!(hits.ordinal(), Some(3));
        assert_eq!(count_label(&hits, &RUNNING), "3 of 3 so far");
        assert_eq!(hits.step((4, 1), false), None);
        assert_eq!(hits.step((4, 1), true).map(|hit| hit.key()), Some((1, 9)));
        assert_eq!(hits.wrap(false).map(|hit| hit.page), Some(1));
        assert_eq!(hits.page_count(), 2);
    }

    #[test]
    fn a_hit_at_the_first_character_is_found_from_its_page() {
        let mut hits = Hits::default();
        hits.add_page(2, vec![hit(2, 0)]);
        assert_eq!(hits.first_from(2).map(|hit| hit.key()), Some((2, 0)));
    }

    #[test]
    fn hits_stop_at_the_cap_and_the_label_says_so() {
        let mut hits = Hits::default();
        let many: Vec<Hit> = (0..HIT_CAP as u32 + 5).map(|at| hit(0, at * 3)).collect();
        hits.add_page(0, many);
        hits.add_page(1, vec![hit(1, 0)]);
        assert_eq!((hits.total(), hits.is_capped()), (HIT_CAP, true));
        assert_eq!(count_label(&hits, &DONE), "100,000+ hits");
    }

    #[test]
    fn the_label_says_when_nothing_was_found_or_pages_were_skipped() {
        let hits = Hits::default();
        assert_eq!(count_label(&hits, &DONE), "No hits");
        assert_eq!(count_label(&hits, &RUNNING), "No hits so far");
        let partial = Progress {
            is_complete: true,
            is_partial: true,
        };
        assert_eq!(count_label(&hits, &partial), "No hits · some pages skipped");
    }

    #[test]
    fn a_page_searched_again_replaces_its_hits() {
        let mut hits = Hits::default();
        hits.add_page(3, vec![hit(3, 1), hit(3, 5)]);
        let before = hits.revision();
        hits.add_page(3, vec![hit(3, 7)]);
        assert_eq!(hits.on(3).len(), 1);
        assert!(hits.revision() > before);
    }
}
