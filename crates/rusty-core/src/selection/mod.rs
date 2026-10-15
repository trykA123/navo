mod pattern;

use crate::Listing;
pub use pattern::{Pattern, PatternError};

#[allow(dead_code, reason = "stubs until the selection is implemented")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    len: u64,
    bits: Vec<u64>,
    anchor: Option<u64>,
}

#[allow(unused_variables, reason = "stubs until the selection is implemented")]
impl Selection {
    #[must_use]
    pub fn new(len: u64) -> Self {
        todo!()
    }

    #[must_use]
    pub fn len(&self) -> u64 {
        todo!()
    }

    #[must_use]
    pub fn count(&self) -> u64 {
        todo!()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        todo!()
    }

    #[must_use]
    pub fn is_marked(&self, row: u64) -> bool {
        todo!()
    }

    #[must_use]
    pub fn marked(&self) -> Vec<u64> {
        todo!()
    }

    #[must_use]
    pub fn anchor(&self) -> Option<u64> {
        todo!()
    }

    pub fn toggle(&mut self, row: u64) -> bool {
        todo!()
    }

    pub fn mark_range(&mut self, to: u64) {
        todo!()
    }

    pub fn mark_pattern(&mut self, listing: &Listing, pattern: &Pattern, folders: bool) {
        todo!()
    }

    pub fn unmark_pattern(&mut self, listing: &Listing, pattern: &Pattern, folders: bool) {
        todo!()
    }

    pub fn mark_all(&mut self, listing: &Listing, folders: bool) {
        todo!()
    }

    pub fn invert(&mut self, listing: &Listing, folders: bool) {
        todo!()
    }

    pub fn clear(&mut self) {
        todo!()
    }
}

#[cfg(test)]
mod tests;
