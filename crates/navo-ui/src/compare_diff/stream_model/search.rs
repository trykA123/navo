//! What the Stream model knows about a search: its progress, match counts and narrowing (D30).
//! The hits themselves arrive as `StreamUpdate::Hits` and live in the model.

use super::StreamModel;

/// How far the app got with the current query.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(in crate::compare_diff) struct Scan {
    pub(super) searched: u32,
    pub(super) total: u32,
    pub(super) done: bool,
}

impl StreamModel {
    /// The tag of the newest query the model waits hits for.
    #[must_use]
    pub fn query(&self) -> u64 {
        self.query
    }

    /// Files searched and files to search, while a search runs; `None` when none runs, or the
    /// app has not yet said how many files there are.
    #[must_use]
    pub fn search_progress(&self) -> Option<(u32, u32)> {
        let scan = self.scan;
        (self.hits.is_some() && !scan.done && scan.total > 0).then_some((scan.searched, scan.total))
    }

    /// Whether the app finished the current search.
    #[must_use]
    pub fn search_done(&self) -> bool {
        self.hits.is_some() && self.scan.done
    }

    /// The changes and the files that match so far; a path-only match counts as a file.
    #[must_use]
    pub fn match_counts(&self) -> (usize, usize) {
        self.hits.as_ref().map_or((0, 0), |hits| {
            (hits.values().map(Vec::len).sum(), hits.len())
        })
    }

    /// The hunks of item `id` that a search kept; `None` when no search narrows the Stream or
    /// the item matched by path only (all its hunks stay).
    #[must_use]
    pub fn matched_hunks(&self, id: u64) -> Option<&[u32]> {
        self.hits
            .as_ref()?
            .get(&id)
            .map(Vec::as_slice)
            .filter(|hunks| !hunks.is_empty())
    }

    /// Stops narrowing and forgets the hits, without a final `Hits` from the app.
    pub fn clear_search(&mut self) {
        if self.hits.take().is_some() {
            self.scan = Scan::default();
            self.fix_cursor();
        }
    }

    /// Starts the same search again under a new tag (the item list changed). The hits shown
    /// stay until the first ones of the new tag replace them, so cards do not blink out.
    pub fn restart_search(&mut self, query: u64) {
        if self.hits.is_some() {
            self.query = query;
            self.scan = Scan::default();
            self.replace_hits = true;
        }
    }

    /// Takes the hits of one `Hits` update: the first of a restarted search replaces the old
    /// ones; later parts hold only the hits since the part before and add to them.
    pub(super) fn take_hits(&mut self, hits: &[(u64, Vec<u32>)]) {
        let replace = std::mem::take(&mut self.replace_hits);
        if let Some(found) = self.hits.as_mut() {
            if replace {
                found.clear();
            }
            found.extend(hits.iter().cloned());
        }
    }
}

#[cfg(test)]
mod tests;
