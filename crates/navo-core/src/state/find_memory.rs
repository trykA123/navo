//! What the find results window remembers between runs.

use crate::Location;
use crate::placement::{MonitorSetup, Placement};

/// The most searches the find results window keeps across starts.
pub const MAX_FINDS: usize = 12;

/// One search the find results window restores: its text and starting folder.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedFind {
    /// The search text, as the user typed it.
    pub text: String,
    /// The folder the search starts from.
    pub base: Location,
}

/// Saved placement, pin and queries for the find results window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FindMemory {
    placement: Vec<Placement>,
    /// Whether the results window stays above every app.
    pub pinned: bool,
    queries: Vec<SavedFind>,
}

impl FindMemory {
    /// Rebuilds the memory from saved parts, applying the same limits as live edits.
    pub(crate) fn from_parts<Q>(
        placement: impl IntoIterator<Item = Placement>,
        pinned: bool,
        queries: Q,
    ) -> Self
    where
        Q: IntoIterator<Item = SavedFind>,
        Q::IntoIter: DoubleEndedIterator,
    {
        Self {
            placement: placement
                .into_iter()
                .take(crate::placement::MAX_PLACEMENTS)
                .collect(),
            pinned,
            queries: newest_queries(queries),
        }
    }

    /// The saved placements, newest first.
    #[must_use]
    pub fn placement(&self) -> &[Placement] {
        &self.placement
    }

    /// Records a placement for the monitor setup it was measured on.
    pub fn remember(&mut self, setup: &MonitorSetup, placement: Placement) {
        crate::placement::remember(&mut self.placement, setup, placement);
    }

    /// The saved queries, oldest first.
    #[must_use]
    pub fn queries(&self) -> &[SavedFind] {
        &self.queries
    }

    /// Replaces the saved queries; blank texts drop, repeats keep their newest
    /// slot, and only the last [`MAX_FINDS`] stay.
    pub fn set_queries(&mut self, queries: Vec<SavedFind>) {
        self.queries = newest_queries(queries);
    }

    /// Whether nothing is saved: no placement, no pin and no query.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.placement.is_empty() && !self.pinned && self.queries.is_empty()
    }
}

/// Walks queries newest to oldest, skipping blanks and older repeats, and stops
/// at [`MAX_FINDS`], so a huge saved list costs only the queries it keeps.
fn newest_queries<Q>(queries: Q) -> Vec<SavedFind>
where
    Q: IntoIterator<Item = SavedFind>,
    Q::IntoIter: DoubleEndedIterator,
{
    let mut kept: Vec<SavedFind> = Vec::with_capacity(MAX_FINDS);
    let mut newest = queries.into_iter().rev();
    while kept.len() < MAX_FINDS {
        let Some(query) = newest.next() else { break };
        let repeat = kept.iter().any(|newer| newer == &query);
        if !repeat && !query.text.trim().is_empty() {
            kept.push(query);
        }
    }
    kept.reverse();
    kept
}
