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
    pub(crate) fn from_parts(
        mut placement: Vec<Placement>,
        pinned: bool,
        queries: Vec<SavedFind>,
    ) -> Self {
        placement.truncate(crate::placement::MAX_PLACEMENTS);
        let mut memory = Self {
            placement,
            pinned,
            queries: Vec::new(),
        };
        memory.set_queries(queries);
        memory
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
        let mut kept: Vec<SavedFind> = Vec::new();
        for query in queries {
            if query.text.trim().is_empty() {
                continue;
            }
            if let Some(index) = kept
                .iter()
                .position(|old| old.text == query.text && old.base == query.base)
            {
                kept.remove(index);
            }
            kept.push(query);
        }
        if let Some(excess) = kept.len().checked_sub(MAX_FINDS) {
            kept.drain(..excess);
        }
        self.queries = kept;
    }

    /// Whether nothing is saved: no placement, no pin and no query.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.placement.is_empty() && !self.pinned && self.queries.is_empty()
    }
}
