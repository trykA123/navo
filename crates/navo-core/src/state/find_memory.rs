//! What the find results window remembers between runs.

use crate::Location;
use crate::placement::{MonitorSetup, Placement};

/// The most searches the find results window keeps across starts.
pub const MAX_FINDS: usize = 12;

/// One search the find results window restores: its text and starting folder.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedFind {
    pub text: String,
    pub base: Location,
}

/// Saved placement, pin and queries for the find results window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FindMemory {
    placement: Vec<Placement>,
    pub pinned: bool,
    queries: Vec<SavedFind>,
}

impl FindMemory {
    #[must_use]
    pub fn placement(&self) -> &[Placement] {
        &self.placement
    }

    pub fn remember(&mut self, setup: &MonitorSetup, placement: Placement) {
        crate::placement::remember(&mut self.placement, setup, placement);
    }

    #[must_use]
    pub fn queries(&self) -> &[SavedFind] {
        &self.queries
    }

    pub fn set_queries(&mut self, queries: Vec<SavedFind>) {
        self.queries = queries
            .into_iter()
            .filter(|query| !query.text.is_empty())
            .collect();
        if let Some(excess) = self.queries.len().checked_sub(MAX_FINDS) {
            self.queries.drain(..excess);
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.placement.is_empty() && !self.pinned && self.queries.is_empty()
    }
}
